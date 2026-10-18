//! Remote image catalog (simplestreams index of images.linuxcontainers.org).
//!
//! The index is fetched server-side, cached for an hour and, when a refresh
//! fails, the last good copy keeps being served.
//!
//! Container / VM availability is derived from the item `ftype`s of a
//! product's latest version, as they really appear in the index:
//! * `squashfs` or `root.tar.xz` (a root filesystem) means a container image;
//! * `disk-kvm.img` (a qcow2 disk) means a virtual-machine image;
//! * `incus.tar.xz` is only the metadata tarball and is present on every
//!   product, VM-only ones included, so it says nothing about the type;
//! * `squashfs.vcdiff` (a delta) is ignored.
use std::sync::Arc;
use std::time::{Duration, Instant};

use serde_json::Value;
use tokio::sync::Mutex;

use crate::model::CatalogImage;

pub const CATALOG_URL: &str = "https://images.linuxcontainers.org/streams/v1/images.json";
pub const CATALOG_TTL: Duration = Duration::from_secs(3600);
const FETCH_TIMEOUT: Duration = Duration::from_secs(20);
/// After a failed refresh, wait this long (at most) before trying again.
const RETRY_BACKOFF: Duration = Duration::from_secs(60);

type Images = Arc<Vec<CatalogImage>>;

#[derive(Clone)]
pub struct Catalog {
    url: String,
    ttl: Duration,
    client: reqwest::Client,
    state: Arc<Mutex<State>>,
}

#[derive(Default)]
struct State {
    /// Last good copy and when it was fetched.
    good: Option<(Instant, Images)>,
    /// Do not refresh before this instant, and why the last attempt failed.
    blocked: Option<(Instant, String)>,
}

impl Default for Catalog {
    fn default() -> Self {
        Catalog::new()
    }
}

impl Catalog {
    pub fn new() -> Catalog {
        Catalog::with_url(CATALOG_URL, CATALOG_TTL)
    }

    /// A catalog backed by another index URL / cache lifetime (tests).
    pub fn with_url(url: impl Into<String>, ttl: Duration) -> Catalog {
        let client = reqwest::Client::builder()
            .timeout(FETCH_TIMEOUT)
            .user_agent(concat!("simple-incus/", env!("CARGO_PKG_VERSION")))
            .build()
            .expect("build http client");
        Catalog { url: url.into(), ttl, client, state: Arc::default() }
    }

    /// amd64 images, cached. A failed refresh serves the last good copy, or
    /// `Err("Catalog unavailable: ...")` when there is none.
    pub async fn get(&self) -> Result<Images, String> {
        let mut st = self.state.lock().await;
        let now = Instant::now();
        if let Some((at, images)) = &st.good {
            if now.duration_since(*at) < self.ttl {
                return Ok(images.clone());
            }
        }
        if let Some((until, why)) = &st.blocked {
            if now < *until {
                return st.good.as_ref().map(|(_, i)| i.clone()).ok_or_else(|| why.clone());
            }
        }
        match self.fetch().await {
            Ok(images) => {
                st.good = Some((Instant::now(), images.clone()));
                st.blocked = None;
                Ok(images)
            }
            Err(e) => {
                let why = format!("Catalog unavailable: {e}");
                st.blocked = Some((Instant::now() + self.ttl.min(RETRY_BACKOFF), why.clone()));
                st.good.as_ref().map(|(_, i)| i.clone()).ok_or(why)
            }
        }
    }

    async fn fetch(&self) -> Result<Images, String> {
        let resp = self.client.get(&self.url).send().await.map_err(|e| e.to_string())?;
        if !resp.status().is_success() {
            return Err(format!("HTTP {}", resp.status()));
        }
        let bytes = resp.bytes().await.map_err(|e| e.to_string())?;
        let json: Value = serde_json::from_slice(&bytes).map_err(|e| format!("invalid index: {e}"))?;
        Ok(Arc::new(parse(&json, "amd64")))
    }
}

fn is_numeric_release(alias: &str) -> bool {
    alias.split('/').nth(1).and_then(|r| r.chars().next()).is_some_and(|c| c.is_ascii_digit())
}

/// The canonical alias: `distro/release` when the product has one (preferring
/// the version number over the codename, so `ubuntu/24.04`), otherwise the
/// alias ending in the variant (`ubuntu/24.04/cloud`), otherwise the first.
fn pick_alias(aliases: &str, variant: &str) -> Option<String> {
    let all: Vec<&str> = aliases.split(',').map(str::trim).filter(|a| !a.is_empty()).collect();
    let segs = |a: &&str| a.split('/').count();
    let mut pool: Vec<&str> = all.iter().copied().filter(|a| segs(a) == 2).collect();
    if pool.is_empty() {
        pool = all.iter().copied().filter(|a| a.rsplit('/').next() == Some(variant)).collect();
    }
    if pool.is_empty() {
        pool = all.clone();
    }
    pool.iter().find(|a| is_numeric_release(a)).or(pool.first()).map(|a| a.to_string())
}

/// Catalog entries of `arch`, one per product, from its latest version (the
/// lexicographically greatest version key). Sorted by alias.
pub fn parse(json: &Value, arch: &str) -> Vec<CatalogImage> {
    let Some(products) = json.get("products").and_then(Value::as_object) else {
        return Vec::new();
    };
    let text = |p: &Value, k: &str| p.get(k).and_then(Value::as_str).unwrap_or_default().to_string();
    let mut out: Vec<CatalogImage> = Vec::new();
    for (key, product) in products {
        if product.get("arch").and_then(Value::as_str) != Some(arch) {
            continue;
        }
        let variant = text(product, "variant");
        let Some(alias) = pick_alias(&text(product, "aliases"), &variant) else { continue };
        let Some(latest) = product
            .get("versions")
            .and_then(Value::as_object)
            .and_then(|v| v.iter().max_by(|a, b| a.0.cmp(b.0)))
            .map(|(_, v)| v)
        else {
            continue;
        };
        let ftypes: Vec<&str> = latest
            .get("items")
            .and_then(Value::as_object)
            .map(|items| items.values().filter_map(|i| i.get("ftype").and_then(Value::as_str)).collect())
            .unwrap_or_default();
        let container = ftypes.iter().any(|f| matches!(*f, "squashfs" | "root.tar.xz"));
        let vm = ftypes.contains(&"disk-kvm.img");
        if !container && !vm || out.iter().any(|i| i.alias == alias) {
            continue;
        }
        let mut parts = alias.split('/');
        let distro = parts.next().unwrap_or_else(|| key.split(':').next().unwrap_or_default()).to_string();
        let release = parts.next().map(str::to_string).unwrap_or_else(|| text(product, "release"));
        let codename = text(product, "release_title");
        let mut description = format!("{} {release}", text(product, "os"));
        if !codename.is_empty() && codename != release {
            description.push_str(&format!(" ({codename})"));
        }
        if variant != "default" {
            description.push_str(&format!(" {variant}"));
        }
        out.push(CatalogImage { alias, distro, release, variant, container, vm, description });
    }
    out.sort_by(|a, b| a.alias.cmp(&b.alias));
    out
}
