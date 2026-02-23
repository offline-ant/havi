//! JSON lines control protocol types.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Client request.
#[derive(Debug, Deserialize)]
pub struct Request {
    pub id: u64,
    pub cmd: String,
    /// Service name for start/stop commands.
    #[serde(default)]
    pub service: Option<String>,
    /// Service-specific arguments.
    #[serde(default)]
    pub args: HashMap<String, serde_json::Value>,
}

/// Server response.
#[derive(Debug, Serialize)]
pub struct Response {
    pub id: u64,
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl Response {
    pub fn ok(id: u64, data: serde_json::Value) -> Self {
        Self { id, ok: true, data: Some(data), error: None }
    }

    pub fn ok_empty(id: u64) -> Self {
        Self { id, ok: true, data: None, error: None }
    }

    pub fn err(id: u64, msg: impl Into<String>) -> Self {
        Self { id, ok: false, data: None, error: Some(msg.into()) }
    }
}

/// Unsolicited event broadcast to all clients.
#[derive(Debug, Serialize)]
pub struct Event {
    pub event: String,
    pub service: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pid: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub port: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,
}
