use std::cell::{Cell, RefCell};
use std::rc::Rc;

use servo::{ClipboardDelegate, StringRequest, WebView};

/// Actions queued by the clipboard delegate for processing in the event loop.
pub(super) enum ClipboardAction {
    Copy(String),
}

/// Shared state between the clipboard delegate and the HAVI event loop.
pub(super) struct ClipboardState {
    /// Paste text set by Makepad's TextInput(was_paste) before Servo reads it.
    pub(super) pending_paste: RefCell<Option<String>>,
    /// Queued copy/clear operations to execute via cx.copy_to_clipboard().
    pub(super) action_queue: RefCell<Vec<ClipboardAction>>,
    /// Last text read from or written to the platform clipboard.
    last_platform_clipboard: RefCell<Option<String>>,
    /// Monotonic revision bumped on every clipboard cache write.
    clipboard_revision: Cell<u64>,
}

impl ClipboardState {
    pub(super) fn new() -> Rc<Self> {
        Rc::new(Self {
            pending_paste: RefCell::new(None),
            action_queue: RefCell::new(Vec::new()),
            last_platform_clipboard: RefCell::new(None),
            clipboard_revision: Cell::new(0),
        })
    }

    fn bump_revision(&self) {
        self.clipboard_revision
            .set(self.clipboard_revision.get().wrapping_add(1));
    }

    fn set_cached_platform_clipboard(&self, text: Option<String>) {
        *self.last_platform_clipboard.borrow_mut() = text;
        self.bump_revision();
    }

    fn cached_platform_clipboard(&self) -> Option<String> {
        self.last_platform_clipboard.borrow().clone()
    }

    pub(super) fn has_clipboard_text(&self) -> bool {
        self.read_platform_clipboard_text()
            .is_some_and(|text| !text.is_empty())
    }

    pub(super) fn set_pending_paste(&self, text: String) {
        *self.pending_paste.borrow_mut() = Some(text.clone());
        self.set_cached_platform_clipboard((!text.is_empty()).then_some(text));
    }

    /// Read from the platform clipboard and update cache when available.
    /// Falls back to the last cached value on read failure.
    pub(super) fn read_platform_clipboard_text(&self) -> Option<String> {
        if let Some(text) = platform_clipboard_read_text() {
            self.set_cached_platform_clipboard(Some(text.clone()));
            return Some(text);
        }
        self.cached_platform_clipboard()
    }
}

/// Clipboard delegate that routes through Makepad instead of arboard.
pub(super) struct MakepadClipboardDelegate {
    pub(super) state: Rc<ClipboardState>,
}

impl ClipboardDelegate for MakepadClipboardDelegate {
    fn clear(&self, _webview: WebView) {
        self.state
            .action_queue
            .borrow_mut()
            .push(ClipboardAction::Copy(String::new()));
        self.state.set_cached_platform_clipboard(None);
    }

    fn get_text(&self, _webview: WebView, request: StringRequest) {
        let pending = self.state.pending_paste.borrow_mut().take();
        let platform = self.state.read_platform_clipboard_text();

        let resolved = match (pending, platform) {
            (Some(pending), Some(platform)) if pending != platform => {
                ::log::warn!(
                    "[havishell] dropping stale pending paste in favor of platform clipboard"
                );
                Some(platform)
            },
            (Some(pending), _) => Some(pending),
            (None, platform) => platform,
        };

        request.success(resolved.unwrap_or_default());
    }

    fn set_text(&self, _webview: WebView, new_contents: String) {
        self.state
            .action_queue
            .borrow_mut()
            .push(ClipboardAction::Copy(new_contents.clone()));
        self.state.set_cached_platform_clipboard(Some(new_contents));
    }
}

#[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
fn platform_clipboard_read_text() -> Option<String> {
    let mut clipboard = arboard::Clipboard::new().ok()?;
    clipboard.get_text().ok()
}

#[cfg(target_os = "android")]
fn platform_clipboard_read_text() -> Option<String> {
    let text = unsafe {
        makepad_widgets::makepad_platform::os::linux::android::android_jni::to_java_paste_from_clipboard()
    };
    (!text.is_empty()).then_some(text)
}

#[cfg(target_os = "ios")]
fn platform_clipboard_read_text() -> Option<String> {
    let text = makepad_widgets::makepad_platform::os::apple::ios::ios_app::with_ios_app(|app| {
        app.paste_from_clipboard()
    });
    (!text.is_empty()).then_some(text)
}

#[cfg(not(any(
    target_os = "linux",
    target_os = "windows",
    target_os = "macos",
    target_os = "android",
    target_os = "ios"
)))]
fn platform_clipboard_read_text() -> Option<String> {
    None
}
