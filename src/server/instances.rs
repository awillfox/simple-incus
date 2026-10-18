//! Instance list and lifecycle server functions.
use leptos::prelude::*;

use crate::model::{Action, InstanceSummary};

#[cfg(feature = "ssr")]
mod imp {
    use std::time::Instant;

    use crate::incus::map::{cpu_limit, cpu_usage_ns, to_summary};
    use crate::incus::{IncusError, Operation};
    use crate::model::{Action, InstanceSummary};
    use crate::state::AppState;

    pub const STOP_WAIT_S: u32 = 30;
    pub const DELETE_WAIT_S: u32 = 60;

    pub async fn list_instances_impl(state: &AppState) -> Result<Vec<InstanceSummary>, IncusError> {
        let raws = state.incus.list_instances().await?;
        let now = Instant::now();
        let mut cpu = state.cpu.lock().unwrap_or_else(|e| e.into_inner());
        let mut live = Vec::new();
        let out = raws
            .iter()
            .map(|r| {
                let mut s = to_summary(r);
                if r.status == "Running" {
                    if let Some(usage) = cpu_usage_ns(r) {
                        live.push(r.name.as_str());
                        s.cpu_pct = cpu.sample_with_limit(&r.name, usage, now, cpu_limit(r));
                    }
                }
                s
            })
            .collect();
        cpu.retain(|n| live.contains(&n));
        Ok(out)
    }

    /// Returns the id of the background operation.
    pub async fn instance_action_impl(state: &AppState, name: &str, action: Action) -> Result<String, IncusError> {
        Ok(state.incus.instance_state_put(name, &action).await?.id)
    }

    /// Ok only for a finished, successful operation. A wait that timed out
    /// leaves the operation still running, which is reported as a timeout.
    fn check(op: Operation, step: &str) -> Result<(), IncusError> {
        if op.status_code == 200 && op.err.is_empty() {
            return Ok(());
        }
        let message = if !op.err.is_empty() {
            op.err
        } else if op.status_code >= 400 {
            format!("Failed to {step}: {}", op.status)
        } else {
            format!("Timed out waiting for {step}")
        };
        Err(IncusError::Api { status: 500, message })
    }

    pub async fn delete_instance_impl(state: &AppState, name: &str, force: bool) -> Result<(), IncusError> {
        let inst = state.incus.get_instance(name).await?;
        if matches!(inst.status.as_str(), "Running" | "Frozen" | "Freezing" | "Starting" | "Stopping") {
            if !force {
                return Err(IncusError::Api {
                    status: 409,
                    message: "Instance is running; enable force to stop and delete".into(),
                });
            }
            let op = state.incus.instance_state_put(name, &Action::Stop { force: true }).await?;
            check(state.incus.wait(&op.id, STOP_WAIT_S).await?, "stop")?;
        }
        let op = state.incus.delete_instance(name).await?;
        check(state.incus.wait(&op.id, DELETE_WAIT_S).await?, "delete")
    }

    pub fn human(e: IncusError) -> leptos::server_fn::ServerFnError {
        match e {
            IncusError::Unreachable(_) => leptos::server_fn::ServerFnError::new("Incus is unreachable"),
            IncusError::Api { message, .. } => leptos::server_fn::ServerFnError::new(message),
            IncusError::Decode(m) => leptos::server_fn::ServerFnError::new(format!("Unexpected response from Incus: {m}")),
        }
    }
}

#[cfg(feature = "ssr")]
pub use imp::{delete_instance_impl, human, instance_action_impl, list_instances_impl};

#[server]
pub async fn list_instances() -> Result<Vec<InstanceSummary>, ServerFnError> {
    let state = expect_context::<crate::state::AppState>();
    list_instances_impl(&state).await.map_err(human)
}

#[server]
pub async fn instance_action(name: String, action: Action) -> Result<String, ServerFnError> {
    let state = expect_context::<crate::state::AppState>();
    instance_action_impl(&state, &name, action).await.map_err(human)
}

#[server]
pub async fn delete_instance(name: String, force: bool) -> Result<(), ServerFnError> {
    let state = expect_context::<crate::state::AppState>();
    delete_instance_impl(&state, &name, force).await.map_err(human)
}
