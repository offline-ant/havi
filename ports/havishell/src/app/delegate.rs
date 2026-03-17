use super::*;
use havi_protocols::client::HpprdClientAsync;
use havi_protocols::credentials::global_credential_store;
use havi_protocols::resolve;
use hppr_client::{Signer, parse_via};
use servo::{
    CameraRequest, EmbedderControl, HpprControlRequest, HpprControlResponse,
    HpprResolveRequest, HpprResolveResponse, HpprResolvedDocument, HpprResolvedMediaSource,
    HpprResolvedSourceRef,
};
use std::sync::{Arc, Mutex, OnceLock};

static DEVTOOLS_BIND: OnceLock<Mutex<Option<String>>> = OnceLock::new();

pub(super) fn set_devtools_bind(bind: String) {
    let cell = DEVTOOLS_BIND.get_or_init(|| Mutex::new(None));
    *cell.lock().unwrap() = Some(bind);
}

pub(super) fn get_devtools_bind() -> Option<String> {
    DEVTOOLS_BIND
        .get()
        .and_then(|cell| cell.lock().ok().and_then(|value| value.clone()))
}

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
    /// Shadow mode transition completed for a tab.
    ShadowModeSet {
        webview_id: WebViewId,
        enabled: bool,
        error: Option<String>,
    },
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
            Self::ShadowModeSet {
                webview_id,
                enabled,
                error,
            } => f
                .debug_struct("ShadowModeSet")
                .field("webview_id", webview_id)
                .field("enabled", enabled)
                .field("error", error)
                .finish(),
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

fn signer_identity_string(signer: &Signer) -> Option<String> {
    match signer {
        Signer::Ring2 { group, signing_key } => Some(format!("ring2:{}#{}", group, signing_key)),
        Signer::Ring1 {
            ring1_name,
            signing_key,
        } => Some(format!("ring1:{}#{}", ring1_name, signing_key)),
        Signer::Ring1Adhoc { token, ring1_name } => {
            Some(format!("ring1:{}#{}", ring1_name, token))
        },
        Signer::Ring2Adhoc {
            credential_input, ..
        } => Some(format!("ring2:{}", credential_input)),
        Signer::Ring2Contextual { username, password } => {
            Some(format!("ring2:/{}#{}", username, password))
        },
        Signer::Anyone { .. } => None,
    }
}

fn map_document(result: resolve::ResolvedDocument) -> HpprResolveResponse {
    HpprResolveResponse::Document(HpprResolvedDocument {
        packet: result.packet.as_bytes().to_vec(),
        endpoint: result.endpoint.to_string(),
        signer: result.signer.as_ref().and_then(signer_identity_string),
        is_repo: result.is_repo,
    })
}

fn map_media(result: resolve::ResolvedMediaSource) -> HpprResolveResponse {
    HpprResolveResponse::Media(HpprResolvedMediaSource {
        packet: result.packet.as_bytes().to_vec(),
        endpoint: result.endpoint.to_string(),
        signer: result.signer.as_ref().and_then(signer_identity_string),
        is_repo: result.is_repo,
        source: HpprResolvedSourceRef {
            endpoint: result.source.endpoint.to_string(),
            signer: result.source.signer.as_ref().and_then(signer_identity_string),
            packet_hash: result.source.packet_hash,
            is_repo: result.source.is_repo,
        },
    })
}

fn parse_source_ref(source: HpprResolvedSourceRef) -> Result<resolve::ResolvedSourceRef, String> {
    let endpoint = parse_via(&source.endpoint).map_err(|error| error.to_string())?;
    let signer = source
        .signer
        .as_deref()
        .map(Signer::parse)
        .transpose()
        .map_err(|error| error.to_string())?;
    Ok(resolve::ResolvedSourceRef {
        endpoint,
        signer,
        packet_hash: source.packet_hash,
        is_repo: source.is_repo,
    })
}

fn resolve_response(request: HpprResolveRequest) -> HpprControlResponse {
    let runtime = match tokio::runtime::Builder::new_current_thread().enable_all().build() {
        Ok(runtime) => runtime,
        Err(err) => {
            return HpprControlResponse::Resolve(HpprResolveResponse::Error(format!(
                "resolve runtime: {err}"
            )));
        },
    };

    runtime.block_on(async move {
        let target = home_repo_target();
        let client = match HpprdClientAsync::new(target) {
            Ok(client) => Arc::new(client),
            Err(err) => {
                return HpprControlResponse::Resolve(HpprResolveResponse::Error(format!(
                    "resolve client: {err}"
                )));
            },
        };
        let creds = global_credential_store();

        let response = match request {
            HpprResolveRequest::Document { url } => {
                match resolve::resolve_document(&url, &client, &creds).await {
                    Ok(result) => map_document(result),
                    Err(error) => HpprResolveResponse::Error(error),
                }
            },
            HpprResolveRequest::Media { url } => {
                match resolve::resolve_media(&url, &client, &creds).await {
                    Ok(result) => map_media(result),
                    Err(error) => HpprResolveResponse::Error(error),
                }
            },
            HpprResolveRequest::ReadBytes {
                source,
                offset,
                length,
            } => match parse_source_ref(source) {
                Ok(source) => match resolve::read_resolved_bytes(&source, &client, offset, length).await {
                    Ok(bytes) => HpprResolveResponse::Bytes(bytes),
                    Err(error) => HpprResolveResponse::Error(error),
                },
                Err(error) => HpprResolveResponse::Error(error),
            },
        };

        HpprControlResponse::Resolve(response)
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
            HpprControlRequest::Resolve(resolve_request) => {
                std::thread::Builder::new()
                    .name("havi-resolve".to_string())
                    .spawn(move || {
                        request.respond(resolve_response(resolve_request));
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
        let bind = format!("127.0.0.1:{}", port);
        set_devtools_bind(bind.clone());
        println!("HAVI_DEVTOOLS={}", bind);
        log!(
            "DEVTOOLS_BIND={} # havi-devtools-cli -p {}",
            bind,
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
