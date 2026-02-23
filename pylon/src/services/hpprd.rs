//! hpprd service definition.

use super::{str_arg, u16_arg};
use std::collections::HashMap;

/// Default hpprd TCP port.
pub const DEFAULT_PORT: u16 = 4777;

pub fn resolve(args: &HashMap<String, serde_json::Value>) -> Result<super::ServiceCommand, String> {
    let program = str_arg(args, "program").unwrap_or_else(|| "hpprd".to_string());
    let mut cmd_args = Vec::new();
    let mut env = HashMap::new();

    if let Some(repo_path) = str_arg(args, "repo_path") {
        cmd_args.push("--path".to_string());
        cmd_args.push(repo_path);
    }

    let bind = if let Some(bind) = str_arg(args, "bind") {
        bind
    } else if let Some(port) = u16_arg(args, "port") {
        format!("127.0.0.1:{}", port)
    } else {
        format!("127.0.0.1:{}", DEFAULT_PORT)
    };
    cmd_args.push("--bind".to_string());
    cmd_args.push(bind);

    if let Some(phc) = str_arg(args, "phc") {
        env.insert("HPPR_PHC".to_string(), phc);
    }

    Ok((program, cmd_args, env, "HPPRD_LISTEN=".to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_keeps_explicit_bind() {
        let mut args = HashMap::new();
        args.insert("bind".to_string(), serde_json::json!("0.0.0.0:4777"));
        let (_, cmd_args, _, _) = resolve(&args).expect("resolve");
        assert_eq!(cmd_args, vec!["--bind", "0.0.0.0:4777"]);
    }

    #[test]
    fn resolve_maps_explicit_port_to_loopback_bind() {
        let mut args = HashMap::new();
        args.insert("port".to_string(), serde_json::json!(4777));
        let (_, cmd_args, _, _) = resolve(&args).expect("resolve");
        assert_eq!(cmd_args, vec!["--bind", "127.0.0.1:4777"]);
    }

    #[test]
    fn resolve_default_uses_standard_port() {
        let args = HashMap::new();
        let (_, cmd_args, _, _) = resolve(&args).expect("resolve");
        assert_eq!(cmd_args, vec!["--bind", "127.0.0.1:4777"]);
    }

    #[test]
    fn resolve_with_repo_path() {
        let mut args = HashMap::new();
        args.insert("repo_path".to_string(), serde_json::json!("/data/repo"));
        let (_, cmd_args, _, _) = resolve(&args).expect("resolve");
        assert_eq!(cmd_args, vec!["--path", "/data/repo", "--bind", "127.0.0.1:4777"]);
    }

    #[test]
    fn resolve_with_phc() {
        let mut args = HashMap::new();
        args.insert("phc".to_string(), serde_json::json!("$argon2id$v=19$m=64,t=3,p=1$"));
        let (_, _, env, _) = resolve(&args).expect("resolve");
        assert_eq!(env.get("HPPR_PHC").unwrap(), "$argon2id$v=19$m=64,t=3,p=1$");
    }
}
