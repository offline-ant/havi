use std::cell::RefCell;
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
}

impl ClipboardState {
    pub(super) fn new() -> Rc<Self> {
        Rc::new(Self {
            pending_paste: RefCell::new(None),
            action_queue: RefCell::new(Vec::new()),
        })
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
    }

    fn get_text(&self, _webview: WebView, request: StringRequest) {
        let text = self.state.pending_paste.borrow_mut().take();
        request.success(text.unwrap_or_default());
    }

    fn set_text(&self, _webview: WebView, new_contents: String) {
        self.state
            .action_queue
            .borrow_mut()
            .push(ClipboardAction::Copy(new_contents));
    }
}
