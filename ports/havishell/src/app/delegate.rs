use super::*;
use havi_protocols::client::HpprdClientAsync;
use havi_protocols::credentials::global_credential_store;
use hppr_client::parse_via;
use servo::{CameraRequest, EmbedderControl, HpprControlRequest, HpprControlResponse};
use std::sync::{Arc, Mutex};

#[derive(Clone)]
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
    /// The cursor should change for a webview.
    CursorChanged {
        webview_id: WebViewId,
        cursor: servo::Cursor,
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
    /// DevTools shell request: navigate a specific WebView to URL.
    DevtoolsSetUrl {
        webview_id: WebViewId,
        url: String,
        response_sender: Sender<Result<String, String>>,
    },
    /// DevTools shell request: activate/switch current tab by WebView.
    DevtoolsActivateWebView {
        webview_id: WebViewId,
        response_sender: Sender<Result<(), String>>,
    },
    /// Servo requests showing a context menu for a webview.
    ContextMenuShow {
        webview_id: WebViewId,
        context_menu: Arc<Mutex<Option<servo::ContextMenu>>>,
    },
    /// Accessibility tree update from a webview.
    AccessibilityUpdate {
        webview_id: WebViewId,
        update: Arc<Mutex<Option<servo::accesskit::TreeUpdate>>>,
    },
    /// Camera request from script.
    CameraRequest(Arc<Mutex<Option<CameraRequest>>>),
}

impl std::fmt::Debug for MakepadServoAction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::None => write!(f, "None"),
            Self::Wake => write!(f, "Wake"),
            Self::TitleChanged { webview_id, title } => f
                .debug_struct("TitleChanged")
                .field("webview_id", webview_id)
                .field("title", title)
                .finish(),
            Self::UrlChanged { webview_id, url } => f
                .debug_struct("UrlChanged")
                .field("webview_id", webview_id)
                .field("url", url)
                .finish(),
            Self::NewFrameReady { webview_id } => f
                .debug_struct("NewFrameReady")
                .field("webview_id", webview_id)
                .finish(),
            Self::CursorChanged { webview_id, cursor } => f
                .debug_struct("CursorChanged")
                .field("webview_id", webview_id)
                .field("cursor", cursor)
                .finish(),
            Self::WebViewClosed { webview_id } => f
                .debug_struct("WebViewClosed")
                .field("webview_id", webview_id)
                .finish(),
            Self::ImeShow { webview_id } => f
                .debug_struct("ImeShow")
                .field("webview_id", webview_id)
                .finish(),
            Self::ImeHide { webview_id } => f
                .debug_struct("ImeHide")
                .field("webview_id", webview_id)
                .finish(),
            Self::WatchGetMode { webview_id, .. } => f
                .debug_struct("WatchGetMode")
                .field("webview_id", webview_id)
                .finish(),
            Self::WatchSetMode {
                webview_id, mode, ..
            } => f
                .debug_struct("WatchSetMode")
                .field("webview_id", webview_id)
                .field("mode", mode)
                .finish(),
            Self::DevtoolsSetUrl {
                webview_id, url, ..
            } => f
                .debug_struct("DevtoolsSetUrl")
                .field("webview_id", webview_id)
                .field("url", url)
                .finish(),
            Self::DevtoolsActivateWebView { webview_id, .. } => f
                .debug_struct("DevtoolsActivateWebView")
                .field("webview_id", webview_id)
                .finish(),
            Self::ContextMenuShow { webview_id, .. } => f
                .debug_struct("ContextMenuShow")
                .field("webview_id", webview_id)
                .finish(),
            Self::AccessibilityUpdate { webview_id, .. } => f
                .debug_struct("AccessibilityUpdate")
                .field("webview_id", webview_id)
                .finish(),
            Self::CameraRequest(_) => write!(f, "CameraRequest"),
        }
    }
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

fn home_repo_target() -> hppr_client::ViaSpec {
    std::env::var("HAVI_HOME")
        .ok()
        .filter(|value| !value.is_empty())
        .and_then(|value| parse_via(&value).ok())
        .unwrap_or_else(havi_protocols::repo_target::get)
}

fn resolve_media_packet_response(url: String) -> HpprControlResponse {
    let runtime = match tokio::runtime::Builder::new_current_thread().enable_all().build() {
        Ok(runtime) => runtime,
        Err(err) => {
            return HpprControlResponse::Error(format!("media resolve runtime: {err}"));
        },
    };

    runtime.block_on(async move {
        let target = home_repo_target();
        let client = match HpprdClientAsync::new(target) {
            Ok(client) => Arc::new(client),
            Err(err) => {
                return HpprControlResponse::Error(format!("media resolve client: {err}"));
            },
        };
        let creds = global_credential_store();
        let page = havi_protocols::pages::hppr::handle_request(&url, &client, &creds).await;
        let Some(packet) = page.hppr_packet else {
            return HpprControlResponse::Error(format!(
                "media resolve returned no packet for {url}"
            ));
        };
        let Some(endpoint) = page.hppr_endpoint else {
            return HpprControlResponse::Error(format!(
                "media resolve returned no endpoint for {url}"
            ));
        };
        let signer = page.hppr_signer;
        HpprControlResponse::ResolvedMediaPacket {
            packet: packet.as_bytes().to_vec(),
            endpoint,
            is_repo: signer.is_none(),
            signer,
        }
    })
}

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

    fn notify_cursor_changed(&self, webview: servo::WebView, cursor: servo::Cursor) {
        Cx::post_action(MakepadServoAction::CursorChanged {
            webview_id: webview.id(),
            cursor,
        });
        SignalToUI::set_ui_signal();
    }

    fn notify_closed(&self, webview: servo::WebView) {
        Cx::post_action(MakepadServoAction::WebViewClosed {
            webview_id: webview.id(),
        });
    }

    fn notify_accessibility_tree_update(
        &self,
        webview: servo::WebView,
        tree_update: servo::accesskit::TreeUpdate,
    ) {
        Cx::post_action(MakepadServoAction::AccessibilityUpdate {
            webview_id: webview.id(),
            update: Arc::new(Mutex::new(Some(tree_update))),
        });
        SignalToUI::set_ui_signal();
    }

    fn handle_control_operation(
        &self,
        _webview: servo::WebView,
        request: servo::ControlOperationRequest,
    ) {
        match request.request.clone() {
            HpprControlRequest::ResolveMediaPacket { url } => {
                std::thread::Builder::new()
                    .name("havi-media-resolve".to_string())
                    .spawn(move || {
                        request.respond(resolve_media_packet_response(url));
                    })
                    .ok();
            },
            _ => {
                request.respond(HpprControlResponse::Error(
                    "Control operations not supported by havishell".to_string(),
                ));
            },
        }
    }

    fn show_embedder_control(&self, webview: servo::WebView, embedder_control: EmbedderControl) {
        match embedder_control {
            EmbedderControl::InputMethod(_) => {
                Cx::post_action(MakepadServoAction::ImeShow {
                    webview_id: webview.id(),
                });
                SignalToUI::set_ui_signal();
            },
            EmbedderControl::ContextMenu(context_menu) => {
                Cx::post_action(MakepadServoAction::ContextMenuShow {
                    webview_id: webview.id(),
                    context_menu: Arc::new(Mutex::new(Some(context_menu))),
                });
                SignalToUI::set_ui_signal();
            },
            _ => {},
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

    fn devtools_set_url(
        &self,
        webview_id: WebViewId,
        url: String,
        response_sender: crossbeam_channel::Sender<Result<String, String>>,
    ) {
        Cx::post_action(MakepadServoAction::DevtoolsSetUrl {
            webview_id,
            url,
            response_sender,
        });
        SignalToUI::set_ui_signal();
    }

    fn devtools_activate_webview(
        &self,
        webview_id: WebViewId,
        response_sender: crossbeam_channel::Sender<Result<(), String>>,
    ) {
        Cx::post_action(MakepadServoAction::DevtoolsActivateWebView {
            webview_id,
            response_sender,
        });
        SignalToUI::set_ui_signal();
    }

    fn handle_camera_request(&self, request: CameraRequest) {
        Cx::post_action(MakepadServoAction::CameraRequest(
            Arc::new(Mutex::new(Some(request))),
        ));
        SignalToUI::set_ui_signal();
    }
}
