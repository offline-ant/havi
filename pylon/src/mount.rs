//! OS-level mount/unmount for hppr-nfs NFS.

/// Default mountpoint.
pub const DEFAULT_MOUNTPOINT: &str = "/mnt/hppr";

/// Mount hppr-nfs NFS share.
///
/// `bind` is the hppr-nfs bind address (e.g. "127.0.0.1").
/// `port` is the hppr-nfs NFS port.
/// `mountpoint` is the local directory to mount on.
pub async fn mount(bind: &str, port: u16, mountpoint: &str) -> Result<(), String> {
    // Create mountpoint if it doesn't exist
    tokio::fs::create_dir_all(mountpoint).await
        .map_err(|e| format!("create {}: {}", mountpoint, e))?;

    let output = tokio::process::Command::new(mount_program())
        .args(mount_args(bind, port, mountpoint))
        .output()
        .await
        .map_err(|e| format!("mount: {}", e))?;

    if output.status.success() {
        Ok(())
    } else {
        // UTF-8 Lossy: OS command stderr, display only
        let stderr = String::from_utf8_lossy(&output.stderr);
        Err(format!("mount failed: {}", stderr.trim()))
    }
}

/// Unmount a mountpoint.
pub async fn unmount(mountpoint: &str) -> Result<(), String> {
    let output = tokio::process::Command::new(umount_program())
        .args(umount_args(mountpoint))
        .output()
        .await
        .map_err(|e| format!("umount: {}", e))?;

    if output.status.success() {
        Ok(())
    } else {
        // UTF-8 Lossy: OS command stderr, display only
        let stderr = String::from_utf8_lossy(&output.stderr);
        Err(format!("umount failed: {}", stderr.trim()))
    }
}

/// Query active NFS mounts from the OS. Returns list of (device, mountpoint).
pub async fn list_nfs_mounts() -> Vec<(String, String)> {
    os_list_nfs_mounts().await.unwrap_or_default()
}

#[cfg(target_os = "linux")]
async fn os_list_nfs_mounts() -> Result<Vec<(String, String)>, String> {
    // /proc/mounts has lines: device mountpoint fstype options ...
    let data = tokio::fs::read_to_string("/proc/mounts").await
        .map_err(|e| format!("read /proc/mounts: {}", e))?;
    Ok(parse_mount_lines(&data, "nfs"))
}

#[cfg(target_os = "macos")]
async fn os_list_nfs_mounts() -> Result<Vec<(String, String)>, String> {
    let output = tokio::process::Command::new("mount")
        .arg("-t").arg("nfs")
        .output().await
        .map_err(|e| format!("mount -t nfs: {}", e))?;
    // macOS mount output: "device on mountpoint (type, opts)"
    let text = String::from_utf8(output.stdout).unwrap_or_default();
    let mut mounts = Vec::new();
    for line in text.lines() {
        let parts: Vec<&str> = line.splitn(4, ' ').collect();
        if parts.len() >= 3 && parts[1] == "on" {
            mounts.push((parts[0].to_string(), parts[2].to_string()));
        }
    }
    Ok(mounts)
}

#[cfg(target_os = "windows")]
async fn os_list_nfs_mounts() -> Result<Vec<(String, String)>, String> {
    let output = tokio::process::Command::new("cmd")
        .args(["/c", "mount"])
        .output().await
        .map_err(|e| format!("mount: {}", e))?;
    let text = String::from_utf8(output.stdout).unwrap_or_default();
    let mut mounts = Vec::new();
    for line in text.lines() {
        // Windows mount output: "\\device\share on X: ..."
        let parts: Vec<&str> = line.splitn(4, ' ').collect();
        if parts.len() >= 3 && parts[1] == "on" {
            mounts.push((parts[0].to_string(), parts[2].to_string()));
        }
    }
    Ok(mounts)
}

#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
async fn os_list_nfs_mounts() -> Result<Vec<(String, String)>, String> {
    Ok(Vec::new())
}

#[cfg(target_os = "linux")]
fn parse_mount_lines(data: &str, fstype: &str) -> Vec<(String, String)> {
    let mut mounts = Vec::new();
    for line in data.lines() {
        let fields: Vec<&str> = line.split_whitespace().collect();
        if fields.len() >= 3 && fields[2] == fstype {
            mounts.push((fields[0].to_string(), fields[1].to_string()));
        }
    }
    mounts
}

// --- Linux ---

#[cfg(target_os = "linux")]
fn mount_program() -> &'static str { "mount" }

#[cfg(target_os = "linux")]
fn mount_args(bind: &str, port: u16, mountpoint: &str) -> Vec<String> {
    vec![
        "-t".into(), "nfs".into(),
        "-o".into(),
        format!("port={p},mountport={p},nfsvers=3,tcp,nolock", p = port),
        format!("{bind}:/"),
        mountpoint.into(),
    ]
}

#[cfg(target_os = "linux")]
fn umount_program() -> &'static str { "umount" }

#[cfg(target_os = "linux")]
fn umount_args(mountpoint: &str) -> Vec<String> {
    vec![mountpoint.into()]
}

// --- macOS ---

#[cfg(target_os = "macos")]
fn mount_program() -> &'static str { "mount" }

#[cfg(target_os = "macos")]
fn mount_args(bind: &str, port: u16, mountpoint: &str) -> Vec<String> {
    vec![
        "-t".into(), "nfs".into(),
        "-o".into(),
        format!("port={p},mountport={p},nfsvers=3,tcp,nolocks,noresvport", p = port),
        format!("{bind}:/"),
        mountpoint.into(),
    ]
}

#[cfg(target_os = "macos")]
fn umount_program() -> &'static str { "umount" }

#[cfg(target_os = "macos")]
fn umount_args(mountpoint: &str) -> Vec<String> {
    vec![mountpoint.into()]
}

// --- Windows ---

#[cfg(target_os = "windows")]
fn mount_program() -> &'static str { "cmd" }

#[cfg(target_os = "windows")]
fn mount_args(bind: &str, port: u16, mountpoint: &str) -> Vec<String> {
    // Windows NFS client uses `mount` from Services for UNIX.
    // Mountpoint should be a drive letter like "H:".
    vec![
        "/c".into(),
        "mount".into(),
        "-o".into(),
        format!("port={p},mtype=soft,nolock", p = port),
        format!("\\\\{bind}\\"),
        mountpoint.into(),
    ]
}

#[cfg(target_os = "windows")]
fn umount_program() -> &'static str { "cmd" }

#[cfg(target_os = "windows")]
fn umount_args(mountpoint: &str) -> Vec<String> {
    vec!["/c".into(), "umount".into(), mountpoint.into()]
}

// --- Fallback for other platforms ---

#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
fn mount_program() -> &'static str { "mount" }

#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
fn mount_args(bind: &str, port: u16, mountpoint: &str) -> Vec<String> {
    vec![
        "-t".into(), "nfs".into(),
        "-o".into(),
        format!("port={p},mountport={p},nfsvers=3,tcp,nolock", p = port),
        format!("{bind}:/"),
        mountpoint.into(),
    ]
}

#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
fn umount_program() -> &'static str { "umount" }

#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
fn umount_args(mountpoint: &str) -> Vec<String> {
    vec![mountpoint.into()]
}
