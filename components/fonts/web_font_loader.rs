/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

use std::fmt::Debug;
use std::sync::Arc;

use base::id::WebViewId;
use servo_url::BrowserUrl;

pub type WebFontLoadCallback = Box<dyn FnOnce(Result<Vec<u8>, ()>) + Send>;

pub trait WebFontLoader: Debug + Send {
    fn load(&self, webview_id: Option<WebViewId>, url: BrowserUrl, callback: WebFontLoadCallback);
}

#[derive(Clone, Debug)]
pub struct WebFontDocumentContext {
    pub loader: Arc<dyn WebFontLoader>,
}
