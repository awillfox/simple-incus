//! Snapshot server functions: list, create, restore, delete.
use leptos::prelude::*;

use crate::model::SnapshotInfo;

#[cfg(feature = "ssr")]
mod imp {
    use crate::incus::{IncusError, Operation};
    use crate::model::SnapshotInfo;
    use crate::server::instances::check_op;
    use crate::state::AppState;
    use crate::ui::format::parse_rfc3339;

    /// Every snapshot operation is waited on for at most this long.
    pub const SNAPSHOT_WAIT_S: u32 = 60;

    fn bad_request(message: impl Into<String>) -> IncusError {
        IncusError::Api { status: 400, message: message.into() }
    }

    /// Incus instance-name rules: letters, digits and hyphens, 1-63 characters,
    /// starting with a letter.
    pub fn validate_snapshot_name(name: &str) -> Result<(), IncusError> {
        let ok = (1..=63).contains(&name.len())
            && name.starts_with(|c: char| c.is_ascii_alphabetic())
            && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '-');
        if ok {
            Ok(())
        } else {
            Err(bad_request(
                "Snapshot name must be 1 to 63 characters: letters, digits and hyphens, starting with a letter",
            ))
        }
    }

    /// `snap-YYYYMMDD-HHMMSS` for a Unix time (UTC).
    pub fn default_snapshot_name(unix_secs: i64) -> String {
        let days = unix_secs.div_euclid(86_400);
        let rem = unix_secs.rem_euclid(86_400);
        // Civil date from days since 1970-01-01 (Howard Hinnant's algorithm).
        let z = days + 719_468;
        let era = z.div_euclid(146_097);
        let doe = z - era * 146_097;
        let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
        let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
        let mp = (5 * doy + 2) / 153;
        let d = doy - (153 * mp + 2) / 5 + 1;
        let m = if mp < 10 { mp + 3 } else { mp - 9 };
        let y = yoe + era * 400 + i64::from(m <= 2);
        format!("snap-{y:04}{m:02}{d:02}-{:02}{:02}{:02}", rem / 3600, rem % 3600 / 60, rem % 60)
    }

    async fn finish(state: &AppState, op: Operation, fail: &str, wait: &str) -> Result<(), IncusError> {
        check_op(state.incus.wait(&op.id, SNAPSHOT_WAIT_S).await?, fail, wait)
    }

    /// Newest first.
    pub async fn list_snapshots_impl(state: &AppState, name: &str) -> Result<Vec<SnapshotInfo>, IncusError> {
        let mut out: Vec<SnapshotInfo> = state
            .incus
            .list_snapshots(name)
            .await?
            .into_iter()
            .map(|s| SnapshotInfo {
                name: s.name.rsplit('/').next().unwrap_or_default().to_string(),
                created_at: s.created_at,
                stateful: s.stateful,
            })
            .collect();
        out.sort_by(|a, b| parse_rfc3339(&b.created_at).cmp(&parse_rfc3339(&a.created_at)));
        Ok(out)
    }

    /// An empty (or blank) name is replaced by a generated `snap-YYYYMMDD-HHMMSS`.
    pub async fn create_snapshot_impl(state: &AppState, name: &str, snap: &str) -> Result<(), IncusError> {
        let snap = snap.trim();
        let snap = if snap.is_empty() {
            default_snapshot_name(crate::ui::format::now_secs())
        } else {
            validate_snapshot_name(snap)?;
            snap.to_string()
        };
        let op = state.incus.create_snapshot(name, &snap).await?;
        finish(state, op, "create snapshot", "snapshot creation").await
    }

    pub async fn restore_snapshot_impl(state: &AppState, name: &str, snap: &str) -> Result<(), IncusError> {
        let op = state.incus.restore_snapshot(name, snap).await?;
        finish(state, op, "restore snapshot", "snapshot restore").await
    }

    pub async fn delete_snapshot_impl(state: &AppState, name: &str, snap: &str) -> Result<(), IncusError> {
        let op = state.incus.delete_snapshot(name, snap).await?;
        finish(state, op, "delete snapshot", "snapshot deletion").await
    }
}

#[cfg(feature = "ssr")]
pub use imp::{
    create_snapshot_impl, default_snapshot_name, delete_snapshot_impl, list_snapshots_impl, restore_snapshot_impl,
    validate_snapshot_name, SNAPSHOT_WAIT_S,
};

#[server]
pub async fn list_snapshots(name: String) -> Result<Vec<SnapshotInfo>, ServerFnError> {
    use crate::server::instances::human;
    let state = expect_context::<crate::state::AppState>();
    list_snapshots_impl(&state, &name).await.map_err(human)
}

/// An empty `snap` generates a `snap-YYYYMMDD-HHMMSS` name.
#[server]
pub async fn create_snapshot(name: String, snap: String) -> Result<(), ServerFnError> {
    use crate::server::instances::human;
    let state = expect_context::<crate::state::AppState>();
    create_snapshot_impl(&state, &name, &snap).await.map_err(human)
}

#[server]
pub async fn restore_snapshot(name: String, snap: String) -> Result<(), ServerFnError> {
    use crate::server::instances::human;
    let state = expect_context::<crate::state::AppState>();
    restore_snapshot_impl(&state, &name, &snap).await.map_err(human)
}

#[server]
pub async fn delete_snapshot(name: String, snap: String) -> Result<(), ServerFnError> {
    use crate::server::instances::human;
    let state = expect_context::<crate::state::AppState>();
    delete_snapshot_impl(&state, &name, &snap).await.map_err(human)
}
