use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

/// Incus reports "unknown" usage (e.g. the disk of a stopped instance) as `-1`;
/// read any negative or missing counter as 0.
fn counter<'de, D: serde::Deserializer<'de>>(d: D) -> Result<u64, D::Error> {
    let v = i128::deserialize(d)?;
    Ok(u64::try_from(v).unwrap_or(0))
}

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
    #[serde(default)]
    pub expanded_devices: HashMap<String, HashMap<String, String>>,
}

/// Snapshot as returned by `/1.0/instances/{name}/snapshots?recursion=1`.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct InstanceSnapshot {
    /// Bare snapshot name (some versions prefix it with `<instance>/`).
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub created_at: String,
    #[serde(default)]
    pub stateful: bool,
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
    #[serde(default, deserialize_with = "counter")]
    pub usage: u64,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct MemoryState {
    #[serde(default, deserialize_with = "counter")]
    pub usage: u64,
    #[serde(default, deserialize_with = "counter")]
    pub usage_peak: u64,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct DiskState {
    #[serde(default, deserialize_with = "counter")]
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
    #[serde(default, deserialize_with = "counter")]
    pub bytes_received: u64,
    #[serde(default, deserialize_with = "counter")]
    pub bytes_sent: u64,
}

/// Image as returned by `/1.0/images?recursion=1`.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct Image {
    #[serde(default)]
    pub fingerprint: String,
    #[serde(default)]
    pub aliases: Vec<ImageAlias>,
    #[serde(default)]
    pub properties: HashMap<String, String>,
    /// `container` or `virtual-machine`.
    #[serde(default, rename = "type")]
    pub kind: String,
    #[serde(default, deserialize_with = "counter")]
    pub size: u64,
    #[serde(default)]
    pub uploaded_at: String,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct ImageAlias {
    #[serde(default)]
    pub name: String,
}

/// The parts of `/1.0/resources` the overview shows.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct Resources {
    #[serde(default)]
    pub cpu: ResourcesCpu,
    #[serde(default)]
    pub memory: ResourcesMemory,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct ResourcesCpu {
    #[serde(default)]
    pub total: u32,
    #[serde(default)]
    pub sockets: Vec<CpuSocket>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct CpuSocket {
    #[serde(default)]
    pub name: String,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct ResourcesMemory {
    #[serde(default, deserialize_with = "counter")]
    pub total: u64,
    #[serde(default, deserialize_with = "counter")]
    pub used: u64,
}

/// Entry of `/1.0/storage-pools?recursion=1`.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct StoragePool {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub driver: String,
}

/// `/1.0/storage-pools/{name}/resources`.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct PoolResources {
    #[serde(default)]
    pub space: PoolSpace,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct PoolSpace {
    #[serde(default, deserialize_with = "counter")]
    pub used: u64,
    #[serde(default, deserialize_with = "counter")]
    pub total: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn negative_usage_reads_as_zero() {
        let st: InstanceState =
            serde_json::from_str(r#"{"status":"Stopped","disk":{"root":{"usage":-1}},"memory":{"usage":-1}}"#).unwrap();
        assert_eq!(st.disk.unwrap()["root"].usage, 0);
        assert_eq!(st.memory.unwrap().usage, 0);
    }
}
