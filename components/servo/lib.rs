/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Servo, the mighty web browser engine from the future.
//!
//! This is a very simple library that wires all of Servo's components together as
//! type `Servo`, along with a Webview implementation, `WebView` to create a working
//! web browser.
//!
//! The `Servo` type is responsible for configuring a `Constellation`, which does the
//! heavy lifting of coordinating all of Servo's internal subsystems, including the
//! `ScriptThread` and the `LayoutThread`, as well maintains the navigation context.

mod clipboard_delegate;
#[cfg(feature = "gamepad")]
mod gamepad_provider;
mod javascript_evaluator;
mod network_manager;
mod proxies;
mod responders;
mod servo;
mod servo_delegate;
mod site_data_manager;
mod user_content_manager;
mod webview;
mod webview_delegate;

pub use crate::script::test;

pub mod background_hang_monitor {
    pub use ::background_hang_monitor::*;
}
pub mod canvas {
    pub use ::canvas::*;
    pub use ::canvas_traits::*;
}
pub mod constellation {
    pub use ::constellation::*;
    pub use ::constellation_traits::*;
}
pub mod devtools {
    pub use ::devtools::*;
    pub use ::devtools_traits::*;
}
pub mod fonts {
    pub use ::fonts::*;
}
pub mod geometry {
    pub use ::servo_geometry::*;
}
pub mod layout {
    pub use ::layout::*;
    pub use ::layout_api::*;
}
pub mod metrics {
    pub use ::metrics::*;
}
pub mod net {
    pub use ::net::*;
    pub use ::net_traits::{AsyncRuntime, ResourceThreads, SiteDescriptor};

    pub mod pub_domains {
        pub use ::net_traits::pub_domains::*;
    }
}
pub mod paint {
    pub use ::paint::*;
    pub use ::paint_api::*;
}
#[path = "pages/mod.rs"]
pub mod pages;
pub mod pixels {
    pub use ::pixels::*;
}
pub mod profile {
    pub use ::profile::*;
}
pub mod script {
    pub use ::script::*;
}
pub mod servo_config {
    pub use ::servo_config::*;
}
pub mod servo_url {
    pub use ::servo_url::*;
}
pub mod storage {
    pub use ::storage::*;
    pub use ::storage_traits::StorageThreads;
}
pub mod timers {
    pub use ::timers::*;
}
pub mod webgpu {
    pub use ::webgpu::*;
}
#[cfg(feature = "webxr")]
pub mod webxr {
    pub use ::webxr::*;
}
pub mod hppr;

pub use crate::constellation::Constellation;
pub use crate::constellation::UnprivilegedContent;

pub mod base {
    pub use ::base::*;
}

pub mod media {
    pub use ::media::*;
    pub use ::servo_media::*;
}

pub mod embedder {
    pub use ::embedder_traits::*;
}

pub mod net_traits {
    pub use ::net_traits::*;
}

pub mod script_traits {
    pub use ::script_traits::*;
}

pub mod constellation_traits {
    pub use ::constellation_traits::*;
}

pub mod fonts_traits {
    pub use ::fonts_traits::*;
}

pub mod storage_traits {
    pub use ::storage_traits::*;
}

pub mod devtools_traits {
    pub use ::devtools_traits::*;
}

pub mod canvas_traits {
    pub use ::canvas_traits::*;
}

pub mod webgpu_traits {
    pub use ::webgpu_traits::*;
}

#[cfg(feature = "webxr")]
pub mod webxr_api {
    pub use ::webxr_api::*;
}

#[cfg(feature = "bluetooth")]
pub mod bluetooth_traits {
    pub use ::bluetooth_traits::*;
}

pub mod background_hang_monitor_api {
    pub use ::background_hang_monitor_api::*;
}

/// Response from a protocol page handler.
#[derive(Debug, Clone)]
pub struct PageResponse {
    /// MIME content type, e.g. "text/html", "application/octet-stream".
    pub content_type: String,
    /// Response body bytes.
    pub body: Vec<u8>,
    /// Optional admin credentials (ring1_name, signing_key) for havi:// pages.
    pub admin_credentials: Option<(String, String)>,
    /// Optional CSP header value for sandbox pages.
    pub csp: Option<String>,
    /// The HPPR packet that produced this response (for document.packet DOM API).
    pub hppr_packet: Option<hppr_client::hppr_packet::Packet>,
    /// Canonical HPPR lookup trace for this page load.
    pub hppr_lookup_trace: Option<embedder_traits::HpprLookupTrace>,
    /// Site Ring1 credentials (ring1_name, signing_key) for window.home.
    pub site_credentials: Option<(String, String)>,
    /// Routed endpoint string for window.route.
    pub hppr_endpoint: Option<String>,
    /// Route signer string for window.route (effective local route auth or anyone).
    pub hppr_signer: Option<String>,
    /// Resolved content authority for the loaded HPPR content.
    pub hppr_content_authority: Option<String>,
    /// Canonical resolved HPPR document source snapshot.
    pub hppr_source: Option<net_traits::HpprDocumentSource>,
}

impl PageResponse {
    pub fn html(body: String) -> Self {
        Self {
            content_type: "text/html".to_string(),
            body: body.into_bytes(),
            admin_credentials: None,
            csp: None,
            hppr_packet: None,
            hppr_lookup_trace: None,
            site_credentials: None,
            hppr_endpoint: None,
            hppr_signer: None,
            hppr_content_authority: None,
            hppr_source: None,
        }
    }

    pub fn new(content_type: impl Into<String>, body: Vec<u8>) -> Self {
        Self {
            content_type: content_type.into(),
            body,
            admin_credentials: None,
            csp: None,
            hppr_packet: None,
            hppr_lookup_trace: None,
            site_credentials: None,
            hppr_endpoint: None,
            hppr_signer: None,
            hppr_content_authority: None,
            hppr_source: None,
        }
    }

    pub fn error(title: &str, message: &str, hint: Option<&str>) -> Self {
        Self::html(hppr::util::render_error_page(title, message, hint))
    }

    pub fn with_hppr_lookup_trace(mut self, trace: embedder_traits::HpprLookupTrace) -> Self {
        self.hppr_lookup_trace = Some(trace);
        self
    }

    pub fn with_admin_credentials(mut self, ring1_name: String, signing_key: String) -> Self {
        self.admin_credentials = Some((ring1_name, signing_key));
        self
    }

    pub fn with_csp(mut self, csp: impl Into<String>) -> Self {
        self.csp = Some(csp.into());
        self
    }

    pub fn with_packet(mut self, packet: hppr_client::hppr_packet::Packet) -> Self {
        self.hppr_packet = Some(packet);
        self
    }
}

// These are Servo's public exports. Everything (apart from a couple exceptions below)
// should be exported at the root. See <https://github.com/servo/servo/issues/18475>.
pub use accesskit;
pub use base::generic_channel::{GenericCallback, GenericSender};
pub use base::id::WebViewId;
pub use embedder_traits::user_contents::UserScript;
pub use embedder_traits::*;
pub use image::RgbaImage;
pub use keyboard_types::{
    Code, CompositionEvent, CompositionState, Key, KeyState, Location, Modifiers, NamedKey,
};
pub use servo_media::player::context::{
    GlApi as MediaGlApi, GlContext as MediaGlContext, NativeDisplay as MediaNativeDisplay,
};
// This API should probably not be exposed in this way. Instead there should be a fully
// fleshed out public domains API if we want to expose it.
pub use net_traits::pub_domains::is_reg_domain;
pub use net_traits::{
    HpprDocumentSource, HpprDocumentSourceSnapshot, clear_hppr_document_source,
    get_hppr_document_source, set_hppr_document_source,
};
pub use net_traits::request::Destination;
// This should be replaced with an API on ServoBuilder.
// See <https://github.com/servo/servo/issues/40950>.
pub use resources;
pub use servo_config::opts::{DiagnosticsLogging, Opts, OutputOptions};
pub use servo_config::prefs::{PrefValue, Preferences, UserAgentPlatform};
pub use servo_config::{opts, prefs};
pub use servo_geometry::{
    DeviceIndependentIntRect, DeviceIndependentPixel, convert_rect_to_css_pixel,
};
pub use servo_url::BrowserUrl;
pub use servo_url::hppr::{HAVIAddress, via_url};
pub use style::Zero;
pub use style_traits::CSSPixel;
pub use webrender_api::units::{
    DeviceIntPoint, DeviceIntRect, DeviceIntSize, DevicePixel, DevicePoint, DeviceVector2D,
};

#[cfg(feature = "gamepad")]
pub use crate::gamepad_provider::{
    GamepadHapticEffectRequest, GamepadHapticEffectRequestType, GamepadProvider,
};
pub use crate::network_manager::{CacheEntry, NetworkManager};
pub use crate::servo::{Servo, ServoBuilder, run_content_process};
pub use crate::servo_delegate::{ServoDelegate, ServoError};
pub use crate::site_data_manager::{SiteData, SiteDataManager, StorageType};
pub use crate::user_content_manager::UserContentManager;
pub use crate::clipboard_delegate::{ClipboardDelegate, StringRequest};
pub use crate::webview::{WebView, WebViewBuilder};
pub use crate::webview_delegate::{
    AlertDialog, AllowOrDenyRequest, AuthenticationRequest, ColorPicker, ConfirmDialog,
    ControlOperationRequest, ContextMenu,
    CreateNewWebViewRequest, EmbedderControl, FilePicker,
    InputMethodControl, NavigationRequest, PermissionRequest, PromptDialog, SelectElement,
    SimpleDialog, WebResourceLoad, WebViewDelegate,
};

// TODO: The protocol handler interface needs to be cleaned and simplified.
pub mod protocol_handler {
    pub use crate::net::fetch::methods::{Data, DoneChannel, FetchContext};
    pub use crate::net::filemanager_thread::FILE_CHUNK_SIZE;
    pub use crate::net::hppr_chunks::{batch_reassemble_chunks, fetch_chunk_blobs, parse_exchange_into_blobs};
    pub use crate::net::hppr_pool::HpprAsyncState;
    pub use crate::net::protocols::{FileProtocolHander, ProtocolHandler, ProtocolRegistry};
    pub use net_traits::NetworkError;
    pub use net_traits::ResourceFetchTiming;
    pub use net_traits::filemanager_thread::RelativePos;
    pub use net_traits::http_status::HttpStatus;
    pub use net_traits::request::Request;
    pub use net_traits::response::{Response, ResponseBody};

    pub use crate::webview_delegate::ProtocolHandlerRegistration;
}
