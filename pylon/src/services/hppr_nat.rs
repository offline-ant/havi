//! hppr-nat service definition.

use std::collections::HashMap;

use super::str_arg;

pub fn resolve(args: &HashMap<String, serde_json::Value>) -> Result<super::ServiceCommand, String> {
    let program = str_arg(args, "program").unwrap_or_else(|| "hppr-nat".to_string());
    let mut cmd_args = Vec::new();
    let env = HashMap::new();

    if let Some(gateway) = str_arg(args, "gateway") {
        cmd_args.push("--gateway".to_string());
        cmd_args.push(gateway);
    }

    if let Some(lifetime) = args.get("lifetime").and_then(|v| v.as_u64()) {
        cmd_args.push("--lifetime".to_string());
        cmd_args.push(lifetime.to_string());
    }

    Ok((program, cmd_args, env, "{\"event\":\"ready\"".to_string()))
}
