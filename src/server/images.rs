//! Local images and the remote catalog.
use leptos::prelude::*;

use crate::model::{CatalogImage, ImageInfo};

/// The catalog search returns at most this many images.
pub const CATALOG_LIMIT: usize = 200;

#[cfg(feature = "ssr")]
mod imp {
    use std::cmp::Ordering;

    use crate::incus::raw;
    use crate::incus::IncusError;
    use crate::model::{CatalogImage, ImageInfo, InstanceKind};
    use crate::server::instances::check_op;
    use crate::state::AppState;
    use crate::ui::format::parse_rfc3339;

    pub const IMAGE_DELETE_WAIT_S: u32 = 60;

    fn describe(r: &raw::Image) -> String {
        let prop = |k: &str| r.properties.get(k).map(String::as_str).filter(|v| !v.is_empty());
        if let Some(d) = prop("description") {
            return d.to_string();
        }
        match (prop("os"), prop("release")) {
            (Some(os), Some(rel)) => format!("{os} {rel}"),
            (Some(only), None) | (None, Some(only)) => only.to_string(),
            (None, None) => r.fingerprint.chars().take(12).collect(),
        }
    }

    /// Newest upload first.
    pub async fn list_images_impl(state: &AppState) -> Result<Vec<ImageInfo>, IncusError> {
        let mut out: Vec<ImageInfo> = state
            .incus
            .list_images()
            .await?
            .into_iter()
            .map(|r| ImageInfo {
                description: describe(&r),
                kind: if r.kind == "virtual-machine" { InstanceKind::Vm } else { InstanceKind::Container },
                aliases: r.aliases.into_iter().map(|a| a.name).collect(),
                fingerprint: r.fingerprint,
                size: r.size,
                uploaded_at: r.uploaded_at,
            })
            .collect();
        out.sort_by(|a, b| parse_rfc3339(&b.uploaded_at).cmp(&parse_rfc3339(&a.uploaded_at)));
        Ok(out)
    }

    /// Incus refuses (and says why) when an instance still uses the image.
    pub async fn delete_image_impl(state: &AppState, fingerprint: &str) -> Result<(), IncusError> {
        let op = state.incus.delete_image(fingerprint).await?;
        check_op(state.incus.wait(&op.id, IMAGE_DELETE_WAIT_S).await?, "delete image", "image deletion")
    }

    /// Natural order: digit runs compare as numbers, text runs as text.
    fn natural(a: &str, b: &str) -> Ordering {
        fn chunks(s: &str) -> Vec<(bool, &str)> {
            let bytes = s.as_bytes();
            let mut out = Vec::new();
            let mut start = 0;
            for i in 1..=bytes.len() {
                if i == bytes.len() || bytes[i].is_ascii_digit() != bytes[start].is_ascii_digit() {
                    out.push((bytes[start].is_ascii_digit(), &s[start..i]));
                    start = i;
                }
            }
            out
        }
        let (ca, cb) = (chunks(a), chunks(b));
        for (x, y) in ca.iter().zip(&cb) {
            let ord = match (x.0, y.0) {
                (true, true) => {
                    let (nx, ny) = (x.1.trim_start_matches('0'), y.1.trim_start_matches('0'));
                    nx.len().cmp(&ny.len()).then(nx.cmp(ny))
                }
                (false, false) => x.1.cmp(y.1),
                (true, false) => Ordering::Less,
                (false, true) => Ordering::Greater,
            };
            if ord != Ordering::Equal {
                return ord;
            }
        }
        ca.len().cmp(&cb.len())
    }

    /// Case-insensitive match on alias, distro, release and description; at
    /// most 200 results, by distro then release (newest first).
    pub async fn catalog_impl(state: &AppState, query: &str) -> Result<Vec<CatalogImage>, String> {
        let all = state.catalog.get().await?;
        let q = query.trim().to_lowercase();
        let mut out: Vec<CatalogImage> = all
            .iter()
            .filter(|i| {
                q.is_empty()
                    || [&i.alias, &i.distro, &i.release, &i.description]
                        .iter()
                        .any(|f| f.to_lowercase().contains(&q))
            })
            .cloned()
            .collect();
        out.sort_by(|a, b| {
            a.distro
                .cmp(&b.distro)
                .then_with(|| natural(&b.release, &a.release))
                .then_with(|| a.alias.cmp(&b.alias))
        });
        out.truncate(super::CATALOG_LIMIT);
        Ok(out)
    }

    #[cfg(test)]
    mod tests {
        use super::natural;
        use std::cmp::Ordering::*;

        #[test]
        fn natural_order() {
            assert_eq!(natural("3.9", "3.21"), Less);
            assert_eq!(natural("24.04", "22.04"), Greater);
            assert_eq!(natural("12", "12"), Equal);
            assert_eq!(natural("edge", "3.21"), Greater);
        }
    }
}

#[cfg(feature = "ssr")]
pub use imp::{catalog_impl, delete_image_impl, list_images_impl};

#[server]
pub async fn list_images() -> Result<Vec<ImageInfo>, ServerFnError> {
    use crate::server::instances::human;
    let state = expect_context::<crate::state::AppState>();
    list_images_impl(&state).await.map_err(human)
}

#[server]
pub async fn delete_image(fingerprint: String) -> Result<(), ServerFnError> {
    use crate::server::instances::human;
    let state = expect_context::<crate::state::AppState>();
    delete_image_impl(&state, &fingerprint).await.map_err(human)
}

#[server]
pub async fn catalog(query: String) -> Result<Vec<CatalogImage>, ServerFnError> {
    let state = expect_context::<crate::state::AppState>();
    catalog_impl(&state, &query).await.map_err(ServerFnError::new)
}
