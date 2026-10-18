use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

/// Incus response envelope.
#[derive(Debug, Deserialize)]
pub(crate) struct Envelope {
    #[serde(default, rename = "type")]
    pub kind: String,
    #[serde(default)]
    pub metadata: Value,
    #[serde(default)]
    pub error: String,
    #[serde(default)]
    pub error_code: u16,
}

/// Background operation as returned by `/1.0/operations/{id}`.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Operation {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub status_code: i32,
    #[serde(default)]
    pub metadata: Value,
    #[serde(default)]
    pub err: String,
    #[serde(default)]
    pub resources: Value,
}

/// Raw instance as returned by `/1.0/instances?recursion=2`.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct Instance {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub status_code: i32,
    #[serde(default, rename = "type")]
    pub kind: String,
    #[serde(default)]
    pub config: HashMap<String, String>,
    #[serde(default)]
    pub expanded_config: HashMap<String, String>,
    #[serde(default)]
    pub profiles: Vec<String>,
    #[serde(default)]
    pub created_at: String,
    #[serde(default)]
    pub state: Option<InstanceState>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct InstanceState {
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub cpu: Option<CpuState>,
    #[serde(default)]
    pub memory: Option<MemoryState>,
    #[serde(default)]
    pub disk: Option<HashMap<String, DiskState>>,
    #[serde(default)]
    pub network: Option<HashMap<String, NetworkState>>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct CpuState {
    #[serde(default)]
    pub usage: u64,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct MemoryState {
    #[serde(default)]
    pub usage: u64,
    #[serde(default)]
    pub usage_peak: u64,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct DiskState {
    #[serde(default)]
    pub usage: u64,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct NetworkState {
    #[serde(default)]
    pub addresses: Vec<Address>,
    #[serde(default)]
    pub counters: Option<NetCounters>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct Address {
    #[serde(default)]
    pub family: String,
    #[serde(default)]
    pub address: String,
    #[serde(default)]
    pub scope: String,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct NetCounters {
    #[serde(default)]
    pub bytes_received: u64,
    #[serde(default)]
    pub bytes_sent: u64,
}
