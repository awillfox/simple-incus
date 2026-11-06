//! Advanced instance creation. The create call returns the Incus operation id
//! as soon as Incus accepts it; the browser follows progress on the event
//! stream, so no HTTP request ever waits for an image download.
use leptos::prelude::*;
use crate::model::CreateRequest;

#[cfg(feature = "ssr")]
mod imp {
    use serde_json::{json, Map, Value};

    use crate::incus::IncusError;
    use crate::model::{validate_name, CreateOptions, CreateRequest, CreateStatus, ImageSource, InstanceKind};
    use crate::state::AppState;

    /// Remote used for catalog images.
    pub const CATALOG_SERVER: &str = "https://images.linuxcontainers.org";

    /// The Incus request body. `root_pool` is the pool for the root device and
    /// is only used when the user overrides the pool or the disk size.
    pub fn create_body(req: &CreateRequest, root_pool: Option<&str>) -> Value {
        let source = match &req.image {
            ImageSource::Catalog { alias } => {
                json!({"type": "image", "alias": alias, "server": CATALOG_SERVER, "protocol": "simplestreams"})
            }
            ImageSource::Local { fingerprint } => json!({"type": "image", "fingerprint": fingerprint}),
        };
        let mut config = Map::new();
        if let Some(cpu) = req.cpu {
            config.insert("limits.cpu".into(), json!(cpu.to_string()));
        }
        if let Some(mem) = req.memory_mib {
            config.insert("limits.memory".into(), json!(format!("{mem}MiB")));
        }
        if let Some(ci) = req.cloud_init.as_deref().filter(|c| !c.trim().is_empty()) {
            config.insert("cloud-init.user-data".into(), json!(ci));
        }
        let mut devices = Map::new();
        let pool = req.pool.as_deref().or(root_pool);
        if req.pool.is_some() || req.disk_gib.is_some() {
            let mut root = Map::new();
            root.insert("type".into(), json!("disk"));
            root.insert("path".into(), json!("/"));
            if let Some(p) = pool {
                root.insert("pool".into(), json!(p));
            }
            if let Some(gib) = req.disk_gib {
                root.insert("size".into(), json!(format!("{gib}GiB")));
            }
            devices.insert("root".into(), Value::Object(root));
        }
        if let Some(net) = req.network.as_deref() {
            devices.insert("eth0".into(), json!({"type": "nic", "name": "eth0", "network": net}));
        }
        let profiles = if req.profiles.is_empty() { vec!["default".to_string()] } else { req.profiles.clone() };
        json!({
            "name": req.name,
            "type": match req.kind { InstanceKind::Container => "container", InstanceKind::Vm => "virtual-machine" },
            "source": source,
            "profiles": profiles,
            "config": config,
            "devices": devices,
            "start": req.start,
        })
    }

    /// Validates, then asks Incus to create; returns the operation id without waiting.
    pub async fn create_instance_impl(state: &AppState, req: &CreateRequest) -> Result<String, IncusError> {
        validate_name(&req.name).map_err(|message| IncusError::Api { status: 400, message })?;
        // A root device override replaces the profile's, so it needs the pool.
        let mut root_pool = None;
        if req.pool.is_none() && req.disk_gib.is_some() {
            let names = if req.profiles.is_empty() { vec!["default".to_string()] } else { req.profiles.clone() };
            for n in names {
                if let Some(p) = state.incus.get_profile(&n).await?.devices.get("root").and_then(|d| d.get("pool")) {
                    root_pool = Some(p.clone());
                }
            }
            if root_pool.is_none() {
                return Err(IncusError::Api {
                    status: 400,
                    message: "The selected profiles have no storage pool; choose a storage pool to set a disk size".to_string(),
                });
            }
        }
        let body = create_body(req, root_pool.as_deref());
        Ok(state.incus.create_instance(&body).await?.id)
    }

    /// Catches up on a create whose final event may have been missed. Incus
    /// forgets finished operations after a few seconds, so a missing operation
    /// is settled by the instance: it must exist and, if a start was requested,
    /// be running.
    pub async fn create_status_impl(state: &AppState, op_id: &str, name: &str, start: bool) -> Result<CreateStatus, IncusError> {
        match state.incus.get_operation(op_id).await {
            Ok(op) => Ok(match op.status.as_str() {
                "Success" => CreateStatus::Done,
                "Failure" | "Cancelled" => {
                    let why = if op.err.is_empty() { op.status } else { op.err };
                    CreateStatus::Failed(why)
                }
                _ => CreateStatus::Running,
            }),
            Err(IncusError::Api { status: 404, .. }) => match state.incus.get_instance(name).await {
                Ok(i) if start && i.status != "Running" => Ok(CreateStatus::Failed(format!(
                    "The instance was created but is not running (status: {})",
                    i.status
                ))),
                Ok(_) => Ok(CreateStatus::Done),
                Err(IncusError::Api { status: 404, .. }) => {
                    Ok(CreateStatus::Failed("Incus did not finish creating the instance".to_string()))
                }
                Err(e) => Err(e),
            },
            Err(e) => Err(e),
        }
    }

    pub async fn create_options_impl(state: &AppState) -> Result<CreateOptions, IncusError> {
        let (profiles, networks, pools) = tokio::try_join!(
            state.incus.list_profiles(),
            state.incus.list_networks(),
            state.incus.storage_pools()
        )?;
        Ok(CreateOptions {
            profiles: profiles.into_iter().map(|p| p.name).collect(),
            networks: networks.into_iter().filter(|n| n.managed).map(|n| n.name).collect(),
            pools: pools.into_iter().map(|p| p.name).collect(),
        })
    }
}

#[cfg(feature = "ssr")]
pub use imp::{create_body, create_instance_impl, create_options_impl, create_status_impl, CATALOG_SERVER};

#[server]
pub async fn create_options() -> Result<crate::model::CreateOptions, ServerFnError> {
    use crate::server::instances::human;
    let state = expect_context::<crate::state::AppState>();
    create_options_impl(&state).await.map_err(human)
}

/// Returns the Incus operation id as soon as Incus accepts the request.
#[server]
pub async fn create_instance(req: CreateRequest) -> Result<String, ServerFnError> {
    use crate::server::instances::human;
    let state = expect_context::<crate::state::AppState>();
    create_instance_impl(&state, &req).await.map_err(human)
}

/// Where a create stands now, for catching up on events the browser missed.
#[server]
pub async fn create_status(op_id: String, name: String, start: bool) -> Result<crate::model::CreateStatus, ServerFnError> {
    use crate::server::instances::human;
    let state = expect_context::<crate::state::AppState>();
    create_status_impl(&state, &op_id, &name, start).await.map_err(human)
}
