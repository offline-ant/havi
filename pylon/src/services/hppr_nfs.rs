//! hppr-nfs service definition.

use std::collections::HashMap;
use super::{str_arg, bool_arg};

pub fn resolve(
    args: &HashMap<String, serde_json::Value>,
) -> Result<super::ServiceCommand, String> {
    let program = str_arg(args, "program").unwrap_or_else(|| "hppr-nfs".to_string());
    let mut cmd_args = Vec::new();
    let env = HashMap::new();

    if let Some(home) = str_arg(args, "home") {
        cmd_args.push("--home".to_string());
        cmd_args.push(home);
    }

    if let Some(signer) = str_arg(args, "signer") {
        cmd_args.push("--signer".to_string());
        cmd_args.push(signer);
    }

    if let Some(root) = str_arg(args, "root") {
        cmd_args.push("--root".to_string());
        cmd_args.push(root);
    }

    if let Some(bind) = str_arg(args, "bind") {
        cmd_args.push("--bind".to_string());
        cmd_args.push(bind);
    }

    if bool_arg(args, "rw") {
        cmd_args.push("--rw".to_string());
    }

    Ok((program, cmd_args, env, "hppr-nfs listening on ".to_string()))
}
