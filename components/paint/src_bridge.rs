use std::cell::RefCell;
use std::collections::{HashMap, VecDeque};
use std::rc::Rc;

use base::id::WebViewId;
use image::RgbaImage;
use webrender_api::units::DeviceRect;

#[derive(Clone)]
pub struct ScreenshotBridge {
    inner: Rc<RefCell<ScreenshotBridgeInner>>,
}

#[derive(Default)]
struct ScreenshotBridgeInner {
    next_request_id: u64,
    pending_requests: VecDeque<ScreenshotBridgeRequest>,
    completed_results: VecDeque<ScreenshotBridgeResult>,
}

pub struct ScreenshotBridgeRequest {
    pub request_id: u64,
    pub webview_id: WebViewId,
}

pub struct ScreenshotBridgeResult {
    pub request_id: u64,
    pub image: RgbaImage,
}

impl Default for ScreenshotBridge {
    fn default() -> Self {
        Self {
            inner: Rc::new(RefCell::new(ScreenshotBridgeInner {
                next_request_id: 1,
                ..Default::default()
            })),
        }
    }
}

impl ScreenshotBridge {
    pub fn allocate_request_id(&self) -> u64 {
        let mut inner = self.inner.borrow_mut();
        let request_id = inner.next_request_id;
        inner.next_request_id = inner.next_request_id.saturating_add(1);
        request_id
    }

    pub fn push_request(&self, request_id: u64, webview_id: WebViewId) {
        self.inner
            .borrow_mut()
            .pending_requests
            .push_back(ScreenshotBridgeRequest {
                request_id,
                webview_id,
            });
    }

    pub fn drain_requests(&self) -> Vec<ScreenshotBridgeRequest> {
        self.inner.borrow_mut().pending_requests.drain(..).collect()
    }

    pub fn push_result(&self, request_id: u64, image: RgbaImage) {
        self.inner
            .borrow_mut()
            .completed_results
            .push_back(ScreenshotBridgeResult { request_id, image });
    }

    pub fn drain_results(&self) -> Vec<ScreenshotBridgeResult> {
        self.inner.borrow_mut().completed_results.drain(..).collect()
    }
}

pub struct PendingScreenshot {
    pub request_id: u64,
    pub webview_id: WebViewId,
    pub rect: Option<DeviceRect>,
    pub callback: Box<dyn FnOnce(Result<RgbaImage, embedder_traits::ScreenshotCaptureError>) + 'static>,
}

pub type PendingScreenshotMap = HashMap<u64, PendingScreenshot>;
