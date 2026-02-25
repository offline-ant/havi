use anyhow::anyhow;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HostStrategy {
    SelfExecProcessRuntime,
    InProcessEmbeddedRuntime,
}

pub(crate) fn strategy_for(is_android: bool) -> HostStrategy {
    if is_android {
        HostStrategy::InProcessEmbeddedRuntime
    } else {
        HostStrategy::SelfExecProcessRuntime
    }
}

pub fn strategy_for_target() -> HostStrategy {
    strategy_for(cfg!(target_os = "android"))
}

pub fn ensure_pylon(
    repo_path: &std::path::Path,
    home: Option<&str>,
) -> anyhow::Result<havi_protocols::pylon::PylonClient> {
    match strategy_for_target() {
        HostStrategy::SelfExecProcessRuntime => {
            havi_protocols::pylon::ensure_pylon_with_self_exec_process_fallback(
                repo_path,
                home,
                pylon::self_exec_process::SELF_EXEC_PROCESS_FEATURE_ENABLED,
            )
        },
        HostStrategy::InProcessEmbeddedRuntime => ensure_pylon_android(repo_path, home),
    }
}

#[cfg(not(target_os = "android"))]
fn ensure_pylon_android(
    _repo_path: &std::path::Path,
    _home: Option<&str>,
) -> anyhow::Result<havi_protocols::pylon::PylonClient> {
    Err(anyhow!(
        "android pylon host strategy requested on non-android build"
    ))
}

#[cfg(target_os = "android")]
fn ensure_pylon_android(
    repo_path: &std::path::Path,
    home: Option<&str>,
) -> anyhow::Result<havi_protocols::pylon::PylonClient> {
    use anyhow::Context;
    use std::sync::OnceLock;

    static STARTED: OnceLock<()> = OnceLock::new();

    if let Some(client) = havi_protocols::pylon::PylonClient::try_connect(repo_path) {
        return Ok(client);
    }

    if STARTED.get().is_none() {
        let repo_path = repo_path.to_path_buf();
        let home = home.map(|s| s.to_string());
        let _ = STARTED.set(());
        std::thread::spawn(move || {
            let rt = tokio::runtime::Runtime::new().expect("android pylon runtime");
            let mode = if let Some(addr) = home {
                pylon::PylonMode::Remote { hpprd_addr: addr }
            } else {
                pylon::PylonMode::Local {
                    repo_path: repo_path.clone(),
                }
            };
            let _ = rt.block_on(pylon::run(None, mode, repo_path));
        });
    }

    for _ in 0..40 {
        if let Some(client) = havi_protocols::pylon::PylonClient::try_connect(repo_path) {
            return Ok(client);
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }

    Err(anyhow!("android pylon thread did not become reachable"))
        .context("failed to initialize pylon control plane on android")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strategy_distinguishes_desktop_and_android() {
        assert_eq!(strategy_for(false), HostStrategy::SelfExecProcessRuntime);
        assert_eq!(strategy_for(true), HostStrategy::InProcessEmbeddedRuntime);
    }
}
