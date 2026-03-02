/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Screenshot support. TODO(havi-render): Wire to Makepad screenshot.

use std::cell::RefCell;

use base::id::WebViewId;
use embedder_traits::ScreenshotCaptureError;
use image::RgbaImage;
use webrender_api::units::DeviceRect;

// TODO(havi-render): Wire screenshot fulfillment to Makepad rendering.
#[allow(dead_code)]
pub(crate) struct ScreenshotRequest {
    webview_id: WebViewId,
    rect: Option<DeviceRect>,
    callback: Box<dyn FnOnce(Result<RgbaImage, ScreenshotCaptureError>) + 'static>,
}

#[derive(Default)]
pub(crate) struct ScreenshotTaker {
    requests: RefCell<Vec<ScreenshotRequest>>,
}

impl ScreenshotTaker {
    pub(crate) fn request_screenshot(
        &self,
        webview_id: WebViewId,
        rect: Option<DeviceRect>,
        callback: Box<dyn FnOnce(Result<RgbaImage, ScreenshotCaptureError>) + 'static>,
    ) {
        self.requests.borrow_mut().push(ScreenshotRequest {
            webview_id,
            rect,
            callback,
        });
    }
}
