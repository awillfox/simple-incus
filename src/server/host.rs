//! Host capacity for the overview page.
use leptos::prelude::*;

use crate::model::HostInfo;

#[cfg(feature = "ssr")]
mod imp {
    use crate::incus::IncusError;
    use crate::model::{HostInfo, PoolUsage};
    use crate::state::AppState;

    /// CPU, memory, storage pools and Incus version. A pool whose usage cannot
    /// be read is still listed, with a zero total (unknown) rather than
    /// failing the whole call.
    pub async fn host_info_impl(state: &AppState) -> Result<HostInfo, IncusError> {
        let c = &state.incus;
        let (res, pools, version) = tokio::join!(c.resources(), c.storage_pools(), c.server_version());
        let (res, pools, incus_version) = (res?, pools?, version?);
        let pools = futures_util::future::join_all(pools.iter().map(|p| async move {
            let space = c.pool_resources(&p.name).await.map(|r| r.space).unwrap_or_default();
            PoolUsage { name: p.name.clone(), driver: p.driver.clone(), used: space.used, total: space.total }
        }))
        .await;
        Ok(HostInfo {
            cpu_model: res.cpu.sockets.first().map(|s| s.name.clone()).unwrap_or_default(),
            cpu_threads: res.cpu.total,
            mem_total: res.memory.total,
            mem_used: res.memory.used,
            pools,
            incus_version,
        })
    }
}

#[cfg(feature = "ssr")]
pub use imp::host_info_impl;

#[server(HostInfoCall)]
pub async fn host_info() -> Result<HostInfo, ServerFnError> {
    use crate::server::instances::human;
    let state = expect_context::<crate::state::AppState>();
    host_info_impl(&state).await.map_err(human)
}
