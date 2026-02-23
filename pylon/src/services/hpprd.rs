//! hpprd service definition.

use super::{str_arg, u16_arg};
use std::collections::HashMap;

pub fn resolve(args: &HashMap<String, serde_json::Value>) -> Result<super::ServiceCommand, String> {
    let program = str_arg(args, "program").unwrap_or_else(|| "hpprd".to_string());
    let mut cmd_args = Vec::new();
    let mut env = HashMap::new();

    if let Some(repo_path) = str_arg(args, "repo_path") {
        cmd_args.push("--path".to_string());
        cmd_args.push(repo_path);
    }

    if let Some(bind) = str_arg(args, "bind") {
        cmd_args.push("--bind".to_string());
        cmd_args.push(bind);
    } else if let Some(port) = u16_arg(args, "port") {
        cmd_args.push("--bind".to_string());
        cmd_args.push(format!("127.0.0.1:{}", port));
    }

    if let Some(phc) = str_arg(args, "phc") {
        env.insert("HPPR_PHC".to_string(), phc);
    }

    Ok((program, cmd_args, env, "HPPRD_BIND=".to_string()))
}
