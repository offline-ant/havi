//! unlokid service definition.

use std::collections::HashMap;
use super::{str_arg, u16_arg, bool_arg};

pub fn resolve(
    args: &HashMap<String, serde_json::Value>,
) -> Result<super::ServiceCommand, String> {
    let program = str_arg(args, "program").unwrap_or_else(|| "unlokid".to_string());
    let mut cmd_args = Vec::new();
    let env = HashMap::new();

    if let Some(hppr) = str_arg(args, "hppr") {
        cmd_args.push("--hppr".to_string());
        cmd_args.push(hppr);
    }

    if let Some(port) = u16_arg(args, "port") {
        cmd_args.push("--port".to_string());
        cmd_args.push(port.to_string());
    }

    if let Some(bind) = str_arg(args, "bind") {
        cmd_args.push("--bind".to_string());
        cmd_args.push(bind);
    }

    if bool_arg(args, "shim") {
        cmd_args.push("--shim".to_string());
    }

    if let Some(ws_url) = str_arg(args, "ws_url") {
        cmd_args.push("--ws-url".to_string());
        cmd_args.push(ws_url);
    }

    Ok((program, cmd_args, env, "hppr-http listening on http://".to_string()))
}
