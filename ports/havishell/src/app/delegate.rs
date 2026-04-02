use super::*;
use libhavi::hppr::client::HpprdClientAsync;
use libhavi::hppr::credentials::global_credential_store;
use libhavi::hppr::resolve;
use hppr_client::{Signer, parse_via};
use libhavi::{
    CameraRequest, ConsoleLogLevel, EmbedderControl, HpprControlRequest, HpprControlResponse,
    HpprEmbedResolveResponse, HpprResolveRequest, HpprResolveResponse, HpprResolvedDocument,
    HpprResolvedMediaSource, HpprResolvedSourceRef,
};
use std::io::Write;
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
    /// A webview load status changed.
    LoadStatusChanged {
        webview_id: WebViewId,
        status: libhavi::LoadStatus,
    },
    /// A webview has new content to paint.
    NewFrameReady {
        webview_id: WebViewId,
        pipeline_id: webrender_api::PipelineId,
    },
    /// The cursor should change for a webview.
    CursorChanged {
        webview_id: WebViewId,
        cursor: libhavi::Cursor,
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
        context_menu: Arc<Mutex<Option<libhavi::ContextMenu>>>,
    },
    /// Accessibility tree update from a webview.
    AccessibilityUpdate {
        webview_id: WebViewId,
        update: Arc<Mutex<Option<libhavi::accesskit::TreeUpdate>>>,
    },
    /// Default browser scrolling should run in the embedder.
    DefaultScrollAction {
        webview_id: WebViewId,
        point: Option<DVec2>,
        delta: DVec2,
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
            Self::LoadStatusChanged { webview_id, status } => f
                .debug_struct("LoadStatusChanged")
                .field("webview_id", webview_id)
                .field("status", status)
                .finish(),
            Self::NewFrameReady {
                webview_id,
                pipeline_id,
            } => f
                .debug_struct("NewFrameReady")
                .field("webview_id", webview_id)
                .field("pipeline_id", pipeline_id)
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
            Self::DefaultScrollAction {
                webview_id,
                point,
                delta,
            } => f
                .debug_struct("DefaultScrollAction")
                .field("webview_id", webview_id)
                .field("point", point)
                .field("delta", delta)
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

fn print_console_message(
    webview_id: Option<WebViewId>,
    level: ConsoleLogLevel,
    message: &str,
) {
    let scope = webview_id
        .map(|id| format!("webview={:?}", id))
        .unwrap_or_else(|| "global".to_string());
    let prefix = format!("[console][{:?}][{}]", level, scope);
    match level {
        ConsoleLogLevel::Warn | ConsoleLogLevel::Error => {
            eprintln!("{} {}", prefix, message);
            let _ = std::io::stderr().flush();
        },
        ConsoleLogLevel::Log |
        ConsoleLogLevel::Debug |
        ConsoleLogLevel::Info |
        ConsoleLogLevel::Trace => {
            println!("{} {}", prefix, message);
            let _ = std::io::stdout().flush();
        },
    }
}

fn home_repo_target() -> hppr_client::ViaSpec {
    std::env::var("HAVI_HOME")
        .ok()
        .filter(|value| !value.is_empty())
        .and_then(|value| parse_via(&value).ok())
        .unwrap_or_else(libhavi::hppr::repo_target::get)
}

fn signer_identity_string(signer: &Signer) -> Option<String> {
    match signer {
        Signer::Ring2 { group, signing_key } => Some(format!("ring2:{}|{}", group, signing_key)),
        Signer::Ring1 {
            ring1_name,
            signing_key,
        } => Some(format!("ring1:{}|{}", ring1_name, signing_key)),
        Signer::Ring1Adhoc { token, ring1_name } => {
            Some(format!("ring1:{}|{}", ring1_name, token))
        },
        Signer::Ring2Adhoc {
            credential_input, ..
        } => Some(format!("ring2:{}", credential_input)),
        Signer::Ring2Contextual { username, password } => {
            Some(format!("ring2:/{}|{}", username, password))
        },
        Signer::Anyone { .. } => None,
    }
}

fn map_document(result: resolve::ResolvedDocument) -> HpprResolveResponse {
    HpprResolveResponse::Document(HpprResolvedDocument {
        packet: result.packet.as_bytes().to_vec(),
        endpoint: result.endpoint.to_string(),
        signer: result.signer.as_ref().and_then(signer_identity_string),
        content_authority: result.content_authority,
        is_repo: result.is_repo,
    })
}

fn map_media(result: resolve::ResolvedMediaSource) -> HpprResolveResponse {
    HpprResolveResponse::Media(HpprResolvedMediaSource {
        packet: result.packet.as_bytes().to_vec(),
        endpoint: result.endpoint.to_string(),
        signer: result.signer.as_ref().and_then(signer_identity_string),
        content_authority: result.content_authority,
        is_repo: result.is_repo,
        source: HpprResolvedSourceRef {
            endpoint: result.source.endpoint.to_string(),
            signer: result.source.signer.as_ref().and_then(signer_identity_string),
            content_authority: result.source.content_authority,
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
        content_authority: source.content_authority,
        packet_hash: source.packet_hash,
        is_repo: source.is_repo,
    })
}

fn with_resolve_runtime<T>(
    on_runtime_error: impl FnOnce(String) -> T,
    f: impl FnOnce(tokio::runtime::Runtime) -> T,
) -> T {
    match tokio::runtime::Builder::new_current_thread().enable_all().build() {
        Ok(runtime) => f(runtime),
        Err(err) => on_runtime_error(format!("resolve runtime: {err}")),
    }
}

fn resolve_response(request: HpprResolveRequest) -> HpprControlResponse {
    with_resolve_runtime(
        |error| HpprControlResponse::Resolve(HpprResolveResponse::Error(error)),
        |runtime| {
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
        },
    )
}

fn embed_resolve_response(url: String) -> HpprControlResponse {
    with_resolve_runtime(
        |error| HpprControlResponse::Error(error),
        |runtime| {
            runtime.block_on(async move {
                let target = home_repo_target();
                let client = match HpprdClientAsync::new(target) {
                    Ok(client) => Arc::new(client),
                    Err(err) => {
                        return HpprControlResponse::Error(format!("embed resolve client: {err}"));
                    },
                };
                let creds = global_credential_store();
                match resolve::resolve_embed_content_authority(&url, &client, &creds).await {
                    Ok(content_authority) => {
                        HpprControlResponse::EmbedResolve(HpprEmbedResolveResponse {
                            content_authority,
                        })
                    },
                    Err(error) => HpprControlResponse::Error(error),
                }
            })
        },
    )
}

impl libhavi::WebViewDelegate for HaviWebViewDelegate {
    fn notify_page_title_changed(&self, webview: libhavi::WebView, title: Option<String>) {
        Cx::post_action(MakepadServoAction::TitleChanged {
            webview_id: webview.id(),
            title,
        });
    }

    fn notify_url_changed(&self, webview: libhavi::WebView, url: libhavi::BrowserUrl) {
        Cx::post_action(MakepadServoAction::UrlChanged {
            webview_id: webview.id(),
            url: url.to_string(),
        });
    }

    fn notify_load_status_changed(&self, webview: libhavi::WebView, status: libhavi::LoadStatus) {
        Cx::post_action(MakepadServoAction::LoadStatusChanged {
            webview_id: webview.id(),
            status,
        });
        SignalToUI::set_ui_signal();
    }

    fn notify_new_frame_ready(
        &self,
        webview: libhavi::WebView,
        pipeline_id: webrender_api::PipelineId,
    ) {
        Cx::post_action(MakepadServoAction::NewFrameReady {
            webview_id: webview.id(),
            pipeline_id,
        });
    }

    fn notify_cursor_changed(&self, webview: libhavi::WebView, cursor: libhavi::Cursor) {
        Cx::post_action(MakepadServoAction::CursorChanged {
            webview_id: webview.id(),
            cursor,
        });
        SignalToUI::set_ui_signal();
    }

    fn notify_closed(&self, webview: libhavi::WebView) {
        Cx::post_action(MakepadServoAction::WebViewClosed {
            webview_id: webview.id(),
        });
    }

    fn notify_scroll_default_action(
        &self,
        webview: libhavi::WebView,
        point: Option<euclid::Point2D<f32, libhavi::CSSPixel>>,
        delta: webrender_api::units::LayoutVector2D,
    ) {
        Cx::post_action(MakepadServoAction::DefaultScrollAction {
            webview_id: webview.id(),
            point: point.map(|point| dvec2(point.x as f64, point.y as f64)),
            delta: dvec2(delta.x as f64, delta.y as f64),
        });
        SignalToUI::set_ui_signal();
    }

    fn notify_accessibility_tree_update(
        &self,
        webview: libhavi::WebView,
        tree_update: libhavi::accesskit::TreeUpdate,
    ) {
        Cx::post_action(MakepadServoAction::AccessibilityUpdate {
            webview_id: webview.id(),
            update: Arc::new(Mutex::new(Some(tree_update))),
        });
        SignalToUI::set_ui_signal();
    }

    fn show_console_message(
        &self,
        webview: libhavi::WebView,
        level: ConsoleLogLevel,
        message: String,
    ) {
        print_console_message(Some(webview.id()), level, &message);
    }

    fn handle_control_operation(
        &self,
        _webview: libhavi::WebView,
        request: libhavi::ControlOperationRequest,
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
            HpprControlRequest::EmbedResolve { url } => {
                std::thread::Builder::new()
                    .name("havi-embed-resolve".to_string())
                    .spawn(move || {
                        request.respond(embed_resolve_response(url));
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

    fn show_embedder_control(&self, webview: libhavi::WebView, embedder_control: EmbedderControl) {
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
        webview: libhavi::WebView,
        _control_id: libhavi::EmbedderControlId,
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

impl libhavi::EventLoopWaker for MakepadEventLoopWaker {
    fn clone_box(&self) -> Box<dyn libhavi::EventLoopWaker> {
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

impl libhavi::ServoDelegate for HaviServoDelegate {
    fn notify_devtools_server_started(&self, port: u16, _token: String) {
        let bind = format!("127.0.0.1:{}", port);
        set_devtools_bind(bind.clone());
        println!("HAVI_DEVTOOLS={}", bind);
        let _ = std::io::stdout().flush();
        log!(
            "DEVTOOLS_BIND={} # havi-devtools-cli -p {}",
            bind,
            port
        );
    }

    fn request_devtools_connection(&self, request: libhavi::AllowOrDenyRequest) {
        request.allow();
    }

    fn show_console_message(&self, level: ConsoleLogLevel, message: String) {
        print_console_message(None, level, &message);
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
