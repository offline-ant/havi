use std::os::unix::net::UnixListener;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

static SOCKET_PATH: OnceLock<PathBuf> = OnceLock::new();

pub fn socket_path() -> Option<PathBuf> {
    SOCKET_PATH.get().cloned()
}

pub fn initialize() {
    if std::env::var_os("HAVI_MAKEPAD_SOCKET").is_some() {
        return;
    }
    let Some(path) = default_socket_path() else {
        return;
    };
    if reserve_socket_path(&path) {
        std::env::set_var("HAVI_MAKEPAD_SOCKET", &path);
        let _ = SOCKET_PATH.set(path);
    }
}

pub fn cleanup() {
    if let Some(path) = SOCKET_PATH.get() {
        let _ = std::fs::remove_file(path);
    }
}

fn default_socket_path() -> Option<PathBuf> {
    let runtime_dir = std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/state/havi")))
        .unwrap_or_else(std::env::temp_dir);
    Some(runtime_dir.join(format!("havi-makepad-{}.sock", std::process::id())))
}

fn reserve_socket_path(path: &Path) -> bool {
    if let Some(parent) = path.parent() {
        if std::fs::create_dir_all(parent).is_err() {
            return false;
        }
    }
    let _ = std::fs::remove_file(path);
    match UnixListener::bind(path) {
        Ok(listener) => {
            drop(listener);
            let _ = std::fs::remove_file(path);
            true
        }
        Err(_) => false,
    }
}
