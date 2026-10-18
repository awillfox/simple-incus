//! Types shared between server and browser (compiles for ssr and hydrate).
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum InstanceKind {
    Container,
    Vm,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum InstanceStatus {
    Running,
    Stopped,
    Frozen,
    Error,
    Other(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InstanceSummary {
    pub name: String,
    pub kind: InstanceKind,
    pub status: InstanceStatus,
    pub image_desc: String,
    pub ipv4: Vec<String>,
    pub ipv6: Vec<String>,
    pub cpu_pct: Option<f32>,
    pub mem_used: u64,
    pub mem_limit: Option<u64>,
    pub disk_used: u64,
    pub net_rx: u64,
    pub net_tx: u64,
    pub created_at: String,
    pub profiles: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SnapshotInfo {
    pub name: String,
    pub created_at: String,
    pub stateful: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PoolUsage {
    pub name: String,
    pub driver: String,
    pub used: u64,
    pub total: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HostInfo {
    pub cpu_model: String,
    pub cpu_threads: u32,
    pub mem_total: u64,
    pub mem_used: u64,
    pub pools: Vec<PoolUsage>,
    pub incus_version: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImageInfo {
    pub fingerprint: String,
    pub aliases: Vec<String>,
    pub description: String,
    pub kind: InstanceKind,
    pub size: u64,
    pub uploaded_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CatalogImage {
    /// e.g. "ubuntu/24.04"
    pub alias: String,
    pub distro: String,
    pub release: String,
    pub variant: String,
    pub container: bool,
    pub vm: bool,
    pub description: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ImageSource {
    Local { fingerprint: String },
    Catalog { alias: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreateRequest {
    pub name: String,
    pub image: ImageSource,
    pub kind: InstanceKind,
    pub cpu: Option<u32>,
    pub memory_mib: Option<u64>,
    pub disk_gib: Option<u64>,
    pub profiles: Vec<String>,
    pub network: Option<String>,
    pub pool: Option<String>,
    pub cloud_init: Option<String>,
    pub start: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum UiEventKind {
    Lifecycle,
    Operation,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UiEvent {
    pub kind: UiEventKind,
    pub instance: Option<String>,
    pub op_id: Option<String>,
    pub progress: Option<String>,
    pub message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Action {
    Start,
    Stop { force: bool },
    Restart { force: bool },
    Freeze,
    Unfreeze,
}

/// Marker so the status frame serialises as `{"kind":"Status","connected":..}`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum StatusKind {
    Status,
}

/// Frames on the browser socket `/ws/events`: a UI event or an Incus
/// connectivity change.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum WsMsg {
    Event(UiEvent),
    Status { kind: StatusKind, connected: bool },
}

impl WsMsg {
    pub fn status(connected: bool) -> WsMsg {
        WsMsg::Status { kind: StatusKind::Status, connected }
    }
}
