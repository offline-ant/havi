/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! HAVI protocol infrastructure — UI-independent HPPR support.
//!
//! This crate provides content generation for all HPPR/HAVI protocol schemes
//! without depending on any particular rendering engine (no servo dependency).
//!
//! Shell embedders (havishell, servoshell) call these functions and feed the
//! result to their rendering layer.

pub mod config;
pub mod client;
pub mod credentials;
pub mod embedded_hpprd;
pub mod local_ip;
pub mod page_shell;
pub mod url;
pub mod util;
pub mod pages;

/// Response from a protocol page handler.
///
/// Content generators return this; the shell converts it to its rendering
/// layer's response type.
#[derive(Debug, Clone)]
pub struct PageResponse {
    /// MIME content type, e.g. "text/html", "application/octet-stream".
    pub content_type: String,
    /// Response body bytes.
    pub body: Vec<u8>,
    /// Optional admin credentials (ring1_name, signing_key) for havi:// pages.
    /// The shell propagates these to the DOM for window.ring0 access.
    pub admin_credentials: Option<(String, String)>,
    /// Optional CSP header value for sandbox pages.
    pub csp: Option<String>,
    /// The HPPR packet that produced this response (for document.packet DOM API).
    pub hppr_packet: Option<hppr_client::hppr_packet::Packet>,
    /// Site Ring1 credentials (ring1_name, signing_key) for window.home.
    pub site_credentials: Option<(String, String)>,
    /// Route endpoint string for window.route.
    pub hppr_endpoint: Option<String>,
    /// Route signer string for window.route (Ring2 identity).
    pub hppr_signer: Option<String>,
}

impl PageResponse {
    /// Create an HTML page response.
    pub fn html(body: String) -> Self {
        Self {
            content_type: "text/html".to_string(),
            body: body.into_bytes(),
            admin_credentials: None,
            csp: None,
            hppr_packet: None,
            site_credentials: None,
            hppr_endpoint: None,
            hppr_signer: None,
        }
    }

    /// Create a response with explicit content type and body.
    pub fn new(content_type: impl Into<String>, body: Vec<u8>) -> Self {
        Self {
            content_type: content_type.into(),
            body,
            admin_credentials: None,
            csp: None,
            hppr_packet: None,
            site_credentials: None,
            hppr_endpoint: None,
            hppr_signer: None,
        }
    }

    /// Create an error page response.
    pub fn error(title: &str, message: &str, hint: Option<&str>) -> Self {
        Self::html(util::render_error_page(title, message, hint))
    }

    /// Set admin credentials for havi:// pages (ring1_name, signing_key).
    pub fn with_admin_credentials(mut self, ring1_name: String, signing_key: String) -> Self {
        self.admin_credentials = Some((ring1_name, signing_key));
        self
    }

    /// Set CSP header for sandboxed pages.
    pub fn with_csp(mut self, csp: impl Into<String>) -> Self {
        self.csp = Some(csp.into());
        self
    }

    /// Attach the HPPR packet that produced this response (for document.packet).
    pub fn with_packet(mut self, packet: hppr_client::hppr_packet::Packet) -> Self {
        self.hppr_packet = Some(packet);
        self
    }
}
