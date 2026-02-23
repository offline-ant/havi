//! hpprd service definition.

use super::{str_arg, u16_arg};
use std::collections::HashMap;

/// First port in the auto-allocation range.
pub const DEFAULT_PORT_START: u16 = 14400;
/// Last port in the auto-allocation range (inclusive).
pub const DEFAULT_PORT_END: u16 = 14450;

/// True when the user provided an explicit bind or port.
pub fn has_explicit_bind(args: &HashMap<String, serde_json::Value>) -> bool {
    args.contains_key("bind") || args.contains_key("port")
}

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
        let offset = u16_arg(args, "_auto_port_offset").unwrap_or(0);
        let port = DEFAULT_PORT_START.checked_add(offset)
            .filter(|p| *p <= DEFAULT_PORT_END)
            .ok_or_else(|| format!(
                "no free hpprd default port in {}..={} (set --bind or --port explicitly)",
                DEFAULT_PORT_START, DEFAULT_PORT_END
            ))?;
        format!("127.0.0.1:{}", port)
    };
    cmd_args.push("--bind".to_string());
    cmd_args.push(bind);

    if let Some(phc) = str_arg(args, "phc") {
        env.insert("HPPR_PHC".to_string(), phc);
    }

    Ok((program, cmd_args, env, "HPPRD_BIND=".to_string()))
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
    fn resolve_default_uses_range_start() {
        let args = HashMap::new();
        let (_, cmd_args, _, _) = resolve(&args).expect("resolve");
        assert_eq!(cmd_args, vec!["--bind", "127.0.0.1:14400"]);
    }

    #[test]
    fn resolve_auto_port_offset() {
        let mut args = HashMap::new();
        args.insert("_auto_port_offset".to_string(), serde_json::json!(5));
        let (_, cmd_args, _, _) = resolve(&args).expect("resolve");
        assert_eq!(cmd_args, vec!["--bind", "127.0.0.1:14405"]);
    }

    #[test]
    fn resolve_auto_port_offset_out_of_range() {
        let mut args = HashMap::new();
        args.insert("_auto_port_offset".to_string(), serde_json::json!(100));
        let err = resolve(&args).unwrap_err();
        assert!(err.contains("no free hpprd default port"));
        assert!(err.contains("--bind"));
    }

    #[test]
    fn has_explicit_bind_detects_bind() {
        let mut args = HashMap::new();
        assert!(!has_explicit_bind(&args));
        args.insert("bind".to_string(), serde_json::json!("0.0.0.0:4777"));
        assert!(has_explicit_bind(&args));
    }

    #[test]
    fn has_explicit_bind_detects_port() {
        let mut args = HashMap::new();
        args.insert("port".to_string(), serde_json::json!(4777));
        assert!(has_explicit_bind(&args));
    }
}
