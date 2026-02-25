#[cfg(target_os = "android")]
use anyhow::anyhow;

/// Ensure pylon is reachable for this HAVI process.
///
/// Host startup policy:
/// - desktop targets: process-host startup (`pylon` binary, with self-exec fallback)
/// - android target: inline host startup (spawn pylon in-process on a thread)
pub fn ensure_pylon(
    repo_dir: &std::path::Path,
    home_addr: Option<&str>,
) -> anyhow::Result<havi_protocols::pylon::PylonClient> {
    #[cfg(target_os = "android")]
    {
        return ensure_pylon_inline_host(repo_dir, home_addr);
    }

    #[cfg(not(target_os = "android"))]
    {
        ensure_pylon_process_host(repo_dir, home_addr)
    }
}

#[cfg(not(target_os = "android"))]
fn ensure_pylon_process_host(
    repo_dir: &std::path::Path,
    home_addr: Option<&str>,
) -> anyhow::Result<havi_protocols::pylon::PylonClient> {
    havi_protocols::pylon::ensure_pylon_with_self_exec_process_fallback(
        repo_dir,
        home_addr,
        cfg!(feature = "embedded-services"),
    )
}

#[cfg(target_os = "android")]
fn ensure_pylon_inline_host(
    repo_dir: &std::path::Path,
    home_addr: Option<&str>,
) -> anyhow::Result<havi_protocols::pylon::PylonClient> {
    use anyhow::Context;
    use std::sync::OnceLock;

    const CONNECT_RETRIES: usize = 40;
    const CONNECT_RETRY_DELAY_MS: u64 = 100;

    static INLINE_HOST_STARTED: OnceLock<()> = OnceLock::new();

    if let Some(client) = havi_protocols::pylon::PylonClient::try_connect(repo_dir) {
        return Ok(client);
    }

    if INLINE_HOST_STARTED.set(()).is_ok() {
        let repo_dir = repo_dir.to_path_buf();
        let home_addr = home_addr.map(|s| s.to_string());
        std::thread::spawn(move || {
            let runtime = tokio::runtime::Runtime::new().expect("android pylon runtime");
            let pylon_mode = if let Some(hpprd_addr) = home_addr {
                pylon::PylonMode::Remote { hpprd_addr }
            } else {
                pylon::PylonMode::Local {
                    repo_path: repo_dir.clone(),
                }
            };
            let _ = runtime.block_on(pylon::run(None, pylon_mode, repo_dir));
        });
    }

    for _ in 0..CONNECT_RETRIES {
        if let Some(client) = havi_protocols::pylon::PylonClient::try_connect(repo_dir) {
            return Ok(client);
        }
        std::thread::sleep(std::time::Duration::from_millis(CONNECT_RETRY_DELAY_MS));
    }

    Err(anyhow!("android pylon thread did not become reachable"))
        .context("failed to initialize pylon control plane on android")
}
