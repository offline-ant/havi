//! lokid service definition.

use std::collections::HashMap;
use super::{str_arg, bool_arg};

pub fn resolve(
    args: &HashMap<String, serde_json::Value>,
) -> Result<super::ServiceCommand, String> {
    let program = str_arg(args, "program").unwrap_or_else(|| "lokid".to_string());
    let mut cmd_args = vec!["serve".to_string()];
    let env = HashMap::new();

    let key = str_arg(args, "key").ok_or("lokid requires 'key' arg")?;
    cmd_args.push("--key".to_string());
    cmd_args.push(key);

    if let Some(bind) = str_arg(args, "bind") {
        cmd_args.push("--bind".to_string());
        cmd_args.push(bind);
    }

    if let Some(name) = str_arg(args, "name") {
        cmd_args.push("--name".to_string());
        cmd_args.push(name);
    }

    if bool_arg(args, "follow") {
        cmd_args.push("--follow".to_string());
    }

    Ok((program, cmd_args, env, "HPPR_BIND=".to_string()))
}
