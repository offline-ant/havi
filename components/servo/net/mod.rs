/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

#![deny(unsafe_code)]

pub mod api;
pub mod async_runtime;
pub mod blob_url_store;
pub mod connector;
pub mod cookie;
pub mod cookie_storage;
mod decoder;
pub mod embedder;
pub mod filemanager_thread;
pub mod filemanager_types;
pub mod http_status;
pub mod mime_classifier;
pub mod policy_container;
pub mod pub_domains;
pub mod quality;
pub mod request;
pub mod response;
mod hosts;
pub mod hppr_chunks;
pub mod hppr_media;
pub mod hppr_pool;
pub mod hsts;
pub mod http_cache;
pub mod http_loader;
pub mod image_cache;
pub mod image_cache_types;
pub mod local_directory_listing;
pub mod protocols;
pub mod request_interceptor;
pub mod resource_thread;
pub mod subresource_integrity;
#[cfg(feature = "test-util")]
pub mod test_util;
mod stream_pub_loader;
mod stream_sub_loader;
mod watch_loader;

/// An implementation of the [Fetch specification](https://fetch.spec.whatwg.org/)
pub mod fetch {
    pub mod cors_cache;
    pub mod fetch_params;
    pub mod headers;
    pub mod methods;
}

/// A module for re-exports of items used in unit tests.
pub mod test {
    pub use super::decoder::DECODER_BUFFER_SIZE;
    pub use super::hosts::parse_hostsfile;
    pub use super::http_loader::HttpState;
}


pub use api::*;
