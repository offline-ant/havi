use super::*;
use servo::EmbedderControl;

#[derive(Clone, Debug)]
pub enum MakepadServoAction {
    None,
    Wake,
    /// A webview's page title changed.
    TitleChanged {
        webview_id: WebViewId,
        title: Option<String>,
    },
    /// A webview's URL changed.
    UrlChanged {
        webview_id: WebViewId,
        url: String,
    },
    /// A webview has new content to paint.
    NewFrameReady {
        webview_id: WebViewId,
    },
    /// A webview was closed by page content (window.close()).
    WebViewClosed {
        webview_id: WebViewId,
    },
    /// A webview requested showing the platform IME for focused editable content.
    ImeShow {
        webview_id: WebViewId,
    },
    /// A webview requested hiding the platform IME.
    ImeHide {
        webview_id: WebViewId,
    },
    /// Request current watch mode for a specific WebView from app state.
    WatchGetMode {
        webview_id: WebViewId,
        response_sender: Sender<String>,
    },
    /// Set watch mode for a specific WebView in app state and return resulting mode.
    WatchSetMode {
        webview_id: WebViewId,
        mode: String,
        response_sender: Sender<String>,
    },
}

impl Default for MakepadServoAction {
    fn default() -> Self {
        Self::None
    }
}

// ---------------------------------------------------------------------------
// WebViewDelegate — forwards Servo webview events to Makepad actions
// ---------------------------------------------------------------------------

pub(super) struct HaviWebViewDelegate;

impl servo::WebViewDelegate for HaviWebViewDelegate {
    fn notify_page_title_changed(&self, webview: servo::WebView, title: Option<String>) {
        Cx::post_action(MakepadServoAction::TitleChanged {
            webview_id: webview.id(),
            title,
        });
    }

    fn notify_url_changed(&self, webview: servo::WebView, url: servo::BrowserUrl) {
        Cx::post_action(MakepadServoAction::UrlChanged {
            webview_id: webview.id(),
            url: url.to_string(),
        });
    }

    fn notify_new_frame_ready(&self, webview: servo::WebView) {
        Cx::post_action(MakepadServoAction::NewFrameReady {
            webview_id: webview.id(),
        });
    }

    fn notify_closed(&self, webview: servo::WebView) {
        Cx::post_action(MakepadServoAction::WebViewClosed {
            webview_id: webview.id(),
        });
    }

    fn show_embedder_control(&self, webview: servo::WebView, embedder_control: EmbedderControl) {
        if matches!(embedder_control, EmbedderControl::InputMethod(_)) {
            Cx::post_action(MakepadServoAction::ImeShow {
                webview_id: webview.id(),
            });
            SignalToUI::set_ui_signal();
        }
    }

    fn hide_embedder_control(
        &self,
        webview: servo::WebView,
        _control_id: servo::EmbedderControlId,
    ) {
        Cx::post_action(MakepadServoAction::ImeHide {
            webview_id: webview.id(),
        });
        SignalToUI::set_ui_signal();
    }
}

// ---------------------------------------------------------------------------
// EventLoopWaker
// ---------------------------------------------------------------------------

pub(super) struct MakepadEventLoopWaker;

impl servo::EventLoopWaker for MakepadEventLoopWaker {
    fn clone_box(&self) -> Box<dyn servo::EventLoopWaker> {
        Box::new(MakepadEventLoopWaker)
    }

    fn wake(&self) {
        Cx::post_action(MakepadServoAction::Wake);
    }
}

// ---------------------------------------------------------------------------
// ServoDelegate — auto-allow devtools connections
// ---------------------------------------------------------------------------

pub(super) struct HaviServoDelegate;

impl servo::ServoDelegate for HaviServoDelegate {
    fn notify_devtools_server_started(&self, port: u16, _token: String) {
        eprintln!("HAVI_DEVTOOLS=127.0.0.1:{}", port);
        log!(
            "DEVTOOLS_BIND=127.0.0.1:{} # havi-devtools-cli -p {}",
            port,
            port
        );
    }

    fn request_devtools_connection(&self, request: servo::AllowOrDenyRequest) {
        request.allow();
    }

    fn watch_get_mode(
        &self,
        webview_id: WebViewId,
        response_sender: crossbeam_channel::Sender<String>,
    ) {
        Cx::post_action(MakepadServoAction::WatchGetMode {
            webview_id,
            response_sender,
        });
        SignalToUI::set_ui_signal();
    }

    fn watch_set_mode(
        &self,
        webview_id: WebViewId,
        mode: String,
        response_sender: crossbeam_channel::Sender<String>,
    ) {
        Cx::post_action(MakepadServoAction::WatchSetMode {
            webview_id,
            mode,
            response_sender,
        });
        SignalToUI::set_ui_signal();
    }
}
