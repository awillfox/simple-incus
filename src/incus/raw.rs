use serde::{Deserialize, Serialize};
use serde_json::Value;

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
