//! Service definitions for each managed process.

pub mod hpprd;
pub mod lokid;
pub mod unlokid;
pub mod hppr_nfs;
pub mod hppr_fuse;

use std::collections::HashMap;

/// Resolved service command: (program, arguments, environment, stdout_port_pattern).
pub type ServiceCommand = (String, Vec<String>, HashMap<String, String>, String);

/// Resolve service start command from name and args.
pub fn resolve(
    name: &str,
    args: &HashMap<String, serde_json::Value>,
) -> Result<ServiceCommand, String> {
    match name {
        "hpprd" => hpprd::resolve(args),
        "lokid" => lokid::resolve(args),
        "unlokid" => unlokid::resolve(args),
        "hppr-nfs" => hppr_nfs::resolve(args),
        "hppr-fuse" => hppr_fuse::resolve(args),
        _ => Err(format!("unknown service: {}", name)),
    }
}

/// Known service names.
pub const SERVICES: &[&str] = &["hpprd", "lokid", "unlokid", "hppr-nfs", "hppr-fuse"];

fn str_arg(args: &HashMap<String, serde_json::Value>, key: &str) -> Option<String> {
    args.get(key).and_then(|v| v.as_str()).map(|s| s.to_string())
}

fn u16_arg(args: &HashMap<String, serde_json::Value>, key: &str) -> Option<u16> {
    args.get(key).and_then(|v| v.as_u64()).map(|n| n as u16)
}

fn bool_arg(args: &HashMap<String, serde_json::Value>, key: &str) -> bool {
    args.get(key).and_then(|v| v.as_bool()).unwrap_or(false)
}
