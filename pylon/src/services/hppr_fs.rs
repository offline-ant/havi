//! hppr-fs service definition.

use std::collections::HashMap;
use super::{str_arg, u16_arg, bool_arg};

pub fn resolve(
    args: &HashMap<String, serde_json::Value>,
) -> Result<super::ServiceCommand, String> {
    let program = str_arg(args, "program").unwrap_or_else(|| "hppr-fs".to_string());
    let mut cmd_args = Vec::new();
    let mut env = HashMap::new();

    if let Some(repo) = str_arg(args, "repo") {
        cmd_args.push("--repo".to_string());
        cmd_args.push(repo);
    }

    if let Some(signer) = str_arg(args, "signer") {
        cmd_args.push("--signer".to_string());
        cmd_args.push(signer);
    }

    if let Some(root) = str_arg(args, "root") {
        cmd_args.push("--root".to_string());
        cmd_args.push(root);
    }

    if let Some(port) = u16_arg(args, "port") {
        cmd_args.push("--port".to_string());
        cmd_args.push(port.to_string());
    }

    if let Some(bind) = str_arg(args, "bind") {
        cmd_args.push("--bind".to_string());
        cmd_args.push(bind);
    }

    if bool_arg(args, "rw") {
        cmd_args.push("--rw".to_string());
    }

    // hppr-fs reads HPPR_REPO from env as fallback
    if let Some(repo) = str_arg(args, "repo") {
        env.insert("HPPR_REPO".to_string(), repo);
    }

    Ok((program, cmd_args, env, "hppr-fs listening on ".to_string()))
}
