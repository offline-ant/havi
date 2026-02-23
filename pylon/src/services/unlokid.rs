//! unlokid service definition.

use std::collections::HashMap;
use super::{str_arg, bool_arg};

pub fn resolve(
    args: &HashMap<String, serde_json::Value>,
) -> Result<super::ServiceCommand, String> {
    let program = str_arg(args, "program").unwrap_or_else(|| "unlokid".to_string());
    let mut cmd_args = Vec::new();
    let env = HashMap::new();

    if let Some(home) = str_arg(args, "home") {
        cmd_args.push("--home".to_string());
        cmd_args.push(home);
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
