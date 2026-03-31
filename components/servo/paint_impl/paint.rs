/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::atomic::{AtomicU32, Ordering};

use base::generic_channel::{GenericSender, RoutedReceiver};
use base::id::{PainterId, PipelineId, WebViewId};
use constellation_traits::{EmbedderToConstellationMessage, WindowSizeType};
use crossbeam_channel::Sender;
use dpi::PhysicalSize;
use embedder_traits::{
    InputEventAndId, InputEventId, InputEventResult, ScreenshotCaptureError,
    ShutdownState, ViewportDetails, WebViewRect,
};
use euclid::{Point2D, Scale, Size2D};
use image::RgbaImage;
use ipc_channel::ipc;
use log::debug;
use smallvec::SmallVec;
use paint_api::{ExternalImageIdRegistry, PaintMessage};
use profile_traits::mem::{
    ProcessReports, ProfilerRegistration, Report, ReportKind,
};
use profile_traits::path;
use profile_traits::time::{self as profile_time};
use servo_config::pref;
use servo_geometry::DeviceIndependentPixel;
use style_traits::CSSPixel;
#[cfg(feature = "webgpu")]
use webgpu::canvas_context::WebGpuExternalImageMap;
use rustc_hash::{FxHashMap, FxHashSet};
use webrender_api::units::{DevicePixel, DevicePoint, LayoutVector2D};
use webrender_api::{FontInstanceKey, FontKey, ImageKey};

use super::InitialPaintState;
use super::screenshot::ScreenshotTaker;
use super::src_bridge::ScreenshotBridge;
use super::touch::TouchHandler;


/// Error type for unknown webview operations.
#[derive(Debug)]
pub struct UnknownWebView;

/// Counter for generating unique resource keys.
static NEXT_KEY_INDEX: AtomicU32 = AtomicU32::new(1);

fn next_key_index() -> u32 {
    NEXT_KEY_INDEX.fetch_add(1, Ordering::Relaxed)
}

pub(crate) struct PaintWebViewCallbacks {
    set_animating: Box<dyn Fn(bool)>,
    notify_scroll_default_action: Box<dyn Fn(Option<Point2D<f32, CSSPixel>>, LayoutVector2D)>,
}

/// [`Paint`] is Servo's rendering subsystem.
///
/// WebRender has been removed. Rendering is now handled by havi-render via Makepad.
/// This struct retains the message routing, resource key generation, WebGPU/WebXR
/// infrastructure, and public API surface.
pub struct Paint {
    /// Current physical viewport size per webview.
    viewport_sizes: RefCell<HashMap<WebViewId, Size2D<u32, DevicePixel>>>,

    /// Embedder-facing webview callbacks keyed by id.
    webviews: RefCell<HashMap<WebViewId, PaintWebViewCallbacks>>,

    /// Webviews with a newly generated frame pending embedder notification.
    pending_frame_notifications: RefCell<FxHashSet<WebViewId>>,

    /// Tracks whether we are in the process of shutting down.
    shutdown_state: Rc<Cell<ShutdownState>>,

    /// The port on which we receive messages.
    paint_receiver: RoutedReceiver<PaintMessage>,

    /// The channel on which messages can be sent to the constellation.
    pub(crate) embedder_to_constellation_sender: Sender<EmbedderToConstellationMessage>,

    /// The [`ExternalImageIdRegistry`] used to generate new `ExternalImageId`s.
    external_image_id_registry: ExternalImageIdRegistry,

    /// The channel on which messages can be sent to the time profiler.
    time_profiler_chan: profile_time::ProfilerChan,

    /// Memory profiler registration handle.
    _mem_profiler_registration: ProfilerRegistration,

    /// Screenshot taker.
    screenshot_taker: ScreenshotTaker,

    /// Shared screenshot bridge to the embedder UI.
    screenshot_bridge: ScreenshotBridge,

    /// Page zoom per webview.
    page_zooms: RefCell<HashMap<WebViewId, f32>>,

    /// HiDPI scale factors per webview.
    hidpi_scale_factors:
        RefCell<HashMap<WebViewId, Scale<f32, DeviceIndependentPixel, DevicePixel>>>,

    /// Some XR devices want to run on the main thread.
    #[cfg(feature = "webxr")]
    webxr_main_thread: RefCell<webxr::MainThreadRegistry>,

    /// External image map for WebGPU.
    #[cfg(feature = "webgpu")]
    webgpu_image_map: std::cell::OnceCell<WebGpuExternalImageMap>,

    /// Pending wheel events awaiting `InputEventHandled` response from script.
    /// Keyed by `InputEventId` so we can match the response to the original event.
    pending_wheel_events: RefCell<PendingWheelEvents>,

    /// Mapping of webview to its root pipeline, updated via `SetFrameTreeForWebView`.
    webview_pipelines: RefCell<HashMap<WebViewId, PipelineId>>,

    /// Touch gesture handler for touch-to-scroll conversion.
    pub(crate) touch_handler: RefCell<TouchHandler>,

    /// Shared image source store for forwarding image updates to the render layer.
    pub(crate) image_source_store: paint_api::SharedImageSourceStore,
}

#[derive(Clone, Copy)]
struct PendingWheelEvent {
    webview_id: WebViewId,
    point: Point2D<f32, CSSPixel>,
    delta: LayoutVector2D,
}

/// Tracks pending wheel events by InputEventId so paint can apply embedder-side
/// default scrolling after DOM dispatch completes.
type PendingWheelEvents = FxHashMap<InputEventId, PendingWheelEvent>;

impl Paint {
    pub fn new(state: InitialPaintState) -> Rc<RefCell<Self>> {
        let registration = state.mem_profiler_chan.prepare_memory_reporting(
            "paint".into(),
            state.paint_proxy.clone(),
            PaintMessage::CollectMemoryReport,
        );

        let external_image_id_registry = ExternalImageIdRegistry::default();

        // TODO: WebXR init needs rework after WebGL removal
        #[cfg(feature = "webxr")]
        let webxr_main_thread = {
            todo!("WebXR init needs rework after WebGL removal — webxr_layer_grand_manager was provided by WebGLComm")
        };

        Rc::new(RefCell::new(Paint {
            viewport_sizes: Default::default(),
            webviews: Default::default(),
            pending_frame_notifications: Default::default(),
            shutdown_state: state.shutdown_state,
            paint_receiver: state.receiver,
            embedder_to_constellation_sender: state.embedder_to_constellation_sender.clone(),
            external_image_id_registry,
            time_profiler_chan: state.time_profiler_chan,
            _mem_profiler_registration: registration,
            screenshot_taker: Default::default(),
            screenshot_bridge: state.screenshot_bridge,
            page_zooms: Default::default(),
            hidpi_scale_factors: Default::default(),
            #[cfg(feature = "webxr")]
            webxr_main_thread: RefCell::new(webxr_main_thread),
            #[cfg(feature = "webgpu")]
            webgpu_image_map: Default::default(),
            pending_wheel_events: Default::default(),
            webview_pipelines: Default::default(),
            touch_handler: RefCell::new(TouchHandler::new()),
            image_source_store: paint_api::SharedImageSourceStore::new(),
        }))
    }

    /// Get a clone of the shared image source store handle for the render layer.
    pub fn image_source_store(&self) -> paint_api::SharedImageSourceStore {
        self.image_source_store.clone()
    }

    pub fn webview_pipelines(&self) -> std::cell::Ref<'_, HashMap<WebViewId, PipelineId>> {
        self.webview_pipelines.borrow()
    }

    pub fn external_image_id_registry(&self) -> ExternalImageIdRegistry {
        self.external_image_id_registry.clone()
    }

    pub fn webxr_running(&self) -> bool {
        #[cfg(feature = "webxr")]
        {
            self.webxr_main_thread.borrow().running()
        }
        #[cfg(not(feature = "webxr"))]
        {
            false
        }
    }

    #[cfg(feature = "webxr")]
    pub fn webxr_main_thread_registry(&self) -> webxr_api::Registry {
        self.webxr_main_thread.borrow().registry()
    }

    #[cfg(feature = "webgpu")]
    pub fn webgpu_image_map(&self) -> WebGpuExternalImageMap {
        self.webgpu_image_map.get_or_init(Default::default).clone()
    }


    pub fn finish_shutting_down(&self) {
        while self.paint_receiver.try_recv().is_ok() {}

        if let Ok((sender, receiver)) = ipc::channel() {
            self.time_profiler_chan
                .send(profile_time::ProfilerMsg::Exit(sender));
            let _ = receiver.recv();
        }
    }

    fn handle_browser_message(&self, msg: PaintMessage) {
        trace_msg_from_constellation!(msg, "{msg:?}");

        match self.shutdown_state() {
            ShutdownState::NotShuttingDown => {},
            ShutdownState::ShuttingDown => {
                self.handle_browser_message_while_shutting_down(msg);
                return;
            },
            ShutdownState::FinishedShuttingDown => return,
        }

        match msg {
            PaintMessage::CollectMemoryReport(sender) => {
                self.collect_memory_report(sender);
            },
            PaintMessage::ChangeRunningAnimationsState(..) => {
                // TODO(havi-render): Forward animation state to Makepad.
            },
            PaintMessage::SetFrameTreeForWebView(webview_id, frame_tree) => {
                self.webview_pipelines
                    .borrow_mut()
                    .insert(webview_id, frame_tree.pipeline.id);
            },
            PaintMessage::SetThrottled(..) => {},
            PaintMessage::PipelineExited(..) => {},
            PaintMessage::ScrollViewportByDelta(webview_id, delta) => {
                self.notify_scroll_default_action(webview_id, None, delta);
            },
            PaintMessage::UpdateEpoch { .. } => {},
            PaintMessage::GenerateFrame(painter_ids) => {
                let webviews = self.webviews.borrow();
                let mut pending = self.pending_frame_notifications.borrow_mut();
                for painter_id in painter_ids {
                    let webview_ids: Vec<_> = webviews
                        .keys()
                        .copied()
                        .filter(|webview_id| PainterId::from(*webview_id) == painter_id)
                        .collect();
                    for webview_id in webview_ids {
                        if let Some(webview) = webviews.get(&webview_id) {
                            (webview.set_animating)(true);
                            pending.insert(webview_id);
                        }
                    }
                }
            },
            PaintMessage::GenerateImageKey(webview_id, result_sender) => {
                self.handle_generate_image_key(webview_id, result_sender);
            },
            PaintMessage::GenerateImageKeysForPipeline(webview_id, pipeline_id) => {
                self.handle_generate_image_keys_for_pipeline(webview_id, pipeline_id);
            },
            PaintMessage::UpdateImages(_painter_id, updates) => {
                self.handle_image_updates(updates);
            },
            PaintMessage::DelayNewFrameForCanvas(_webview_id, pipeline_id, _epoch, _image_keys) => {
                // HAVI's Makepad-based renderer doesn't do async canvas image uploads.
                // Immediately acknowledge so the script thread isn't blocked.
                let _ = self.embedder_to_constellation_sender.send(
                    EmbedderToConstellationMessage::NoLongerWaitingOnAsynchronousImageUpdates(
                        vec![pipeline_id],
                    ),
                );
            },
            PaintMessage::AddFont(..) => {
                // TODO(havi-render): Forward font data to Makepad font loader.
            },
            PaintMessage::AddSystemFont(..) => {
                // TODO(havi-render): Forward system font to Makepad font loader.
            },
            PaintMessage::AddFontInstance(..) => {
                // TODO(havi-render): Forward font instance to Makepad.
            },
            PaintMessage::RemoveFonts(..) => {},
            PaintMessage::GenerateFontKeys(
                number_of_font_keys,
                number_of_font_instance_keys,
                result_sender,
                painter_id,
            ) => {
                self.handle_generate_font_keys(
                    number_of_font_keys,
                    number_of_font_instance_keys,
                    result_sender,
                    painter_id,
                );
            },
            PaintMessage::Viewport(..) => {},
            PaintMessage::ScreenshotReadinessReponse(..) => {},
            PaintMessage::SendLCPCandidate(..) => {},
            PaintMessage::EnableLCPCalculation(..) => {},
        }
    }

    pub fn remove_webview(&mut self, webview_id: WebViewId) {
        self.viewport_sizes.borrow_mut().remove(&webview_id);
        self.webviews.borrow_mut().remove(&webview_id);
        self.page_zooms.borrow_mut().remove(&webview_id);
        self.hidpi_scale_factors.borrow_mut().remove(&webview_id);
        self.webview_pipelines.borrow_mut().remove(&webview_id);
        self.screenshot_taker.fail_webview(webview_id);
        // TODO(havi-render): Clean up webview state.
    }

    fn collect_memory_report(&self, sender: profile_traits::mem::ReportsChan) {
        let reports = vec![
            Report {
                path: path!["paint", "placeholder"],
                kind: ReportKind::ExplicitJemallocHeapSize,
                size: 0,
            },
        ];
        sender.send(ProcessReports::new(reports));
    }

    fn handle_browser_message_while_shutting_down(&self, msg: PaintMessage) {
        match msg {
            PaintMessage::PipelineExited(..) => {},
            PaintMessage::GenerateImageKey(webview_id, result_sender) => {
                self.handle_generate_image_key(webview_id, result_sender);
            },
            PaintMessage::GenerateImageKeysForPipeline(webview_id, pipeline_id) => {
                self.handle_generate_image_keys_for_pipeline(webview_id, pipeline_id);
            },
            PaintMessage::GenerateFontKeys(
                number_of_font_keys,
                number_of_font_instance_keys,
                result_sender,
                painter_id,
            ) => {
                self.handle_generate_font_keys(
                    number_of_font_keys,
                    number_of_font_instance_keys,
                    result_sender,
                    painter_id,
                );
            },
            _ => {
                debug!("Ignoring message ({:?} while shutting down", msg);
            },
        }
    }

    pub fn add_webview(
        &self,
        webview_id: WebViewId,
        set_animating: Box<dyn Fn(bool)>,
        notify_scroll_default_action: Box<dyn Fn(Option<Point2D<f32, CSSPixel>>, LayoutVector2D)>,
        viewport_details: ViewportDetails,
    ) {
        let physical_size = (viewport_details.size * viewport_details.hidpi_scale_factor)
            .to_u32()
            .cast_unit();
        self.viewport_sizes.borrow_mut().insert(webview_id, physical_size);
        self.webviews.borrow_mut().insert(
            webview_id,
            PaintWebViewCallbacks {
                set_animating,
                notify_scroll_default_action,
            },
        );
        // TODO(havi-render): Register webview with Makepad renderer.
    }

    pub fn show_webview(&self, _webview_id: WebViewId) -> Result<(), UnknownWebView> {
        Ok(())
    }

    pub fn hide_webview(&self, _webview_id: WebViewId) -> Result<(), UnknownWebView> {
        Ok(())
    }

    pub fn set_hidpi_scale_factor(
        &self,
        webview_id: WebViewId,
        new_scale_factor: Scale<f32, DeviceIndependentPixel, DevicePixel>,
    ) {
        if self.shutdown_state() != ShutdownState::NotShuttingDown {
            return;
        }
        self.hidpi_scale_factors
            .borrow_mut()
            .insert(webview_id, new_scale_factor);
    }

    pub fn resize_webview(&self, webview_id: WebViewId, new_size: PhysicalSize<u32>) {
        if self.shutdown_state() != ShutdownState::NotShuttingDown {
            return;
        }

        self.viewport_sizes.borrow_mut().insert(
            webview_id,
            Size2D::new(new_size.width, new_size.height),
        );

        let hidpi_scale_factor = self
            .hidpi_scale_factors
            .borrow()
            .get(&webview_id)
            .copied()
            .unwrap_or_else(Scale::identity);
        let scaled_viewport_size =
            Size2D::<f32, DevicePixel>::new(new_size.width as f32, new_size.height as f32)
                / hidpi_scale_factor;
        let viewport_details = ViewportDetails {
            size: scaled_viewport_size / Scale::new(1.0),
            hidpi_scale_factor: Scale::new(hidpi_scale_factor.0),
        };
        let _ = self
            .embedder_to_constellation_sender
            .send(EmbedderToConstellationMessage::ChangeViewportDetails(
                webview_id,
                viewport_details,
                WindowSizeType::Resize,
            ));
    }

    pub fn set_page_zoom(&self, webview_id: WebViewId, new_zoom: f32) {
        if self.shutdown_state() != ShutdownState::NotShuttingDown {
            return;
        }
        let clamped = new_zoom.clamp(0.1, 10.0);
        self.page_zooms.borrow_mut().insert(webview_id, clamped);
    }

    pub fn page_zoom(&self, webview_id: WebViewId) -> f32 {
        *self.page_zooms.borrow().get(&webview_id).unwrap_or(&1.0)
    }

    /// Get the message receiver for this [`Paint`].
    pub fn receiver(&self) -> &RoutedReceiver<PaintMessage> {
        &self.paint_receiver
    }

    #[servo_tracing::instrument(skip_all)]
    pub fn handle_messages(&self, messages: Vec<PaintMessage>) {
        for message in messages {
            self.handle_browser_message(message);
            if self.shutdown_state() == ShutdownState::FinishedShuttingDown {
                return;
            }
        }
    }

    #[servo_tracing::instrument(skip_all)]
    pub fn perform_updates(&self) -> Vec<WebViewId> {
        if self.shutdown_state() == ShutdownState::FinishedShuttingDown {
            return Vec::new();
        }

        self.screenshot_taker
            .fulfill_completed(&self.screenshot_bridge);

        #[cfg(feature = "webxr")]
        self.webxr_main_thread.borrow_mut().run_one_frame();

        self.pending_frame_notifications.borrow_mut().drain().collect()
    }

    pub fn notify_input_event(&self, webview_id: WebViewId, event: InputEventAndId) {
        if let embedder_traits::InputEvent::Wheel(wheel_event) = event.event {
            let dpp = self.device_pixels_per_page_pixel(webview_id);
            let point = match wheel_event.point {
                embedder_traits::WebViewPoint::Device(point) => point / dpp,
                embedder_traits::WebViewPoint::Page(point) => point,
            };
            let line_height: f32 = 16.0;
            let page_height: f32 = 800.0;
            let delta = match wheel_event.delta.mode {
                embedder_traits::WheelMode::DeltaPixel => LayoutVector2D::new(
                    -wheel_event.delta.x as f32 / dpp.get(),
                    -wheel_event.delta.y as f32 / dpp.get(),
                ),
                embedder_traits::WheelMode::DeltaLine => LayoutVector2D::new(
                    -wheel_event.delta.x as f32 * line_height,
                    -wheel_event.delta.y as f32 * line_height,
                ),
                embedder_traits::WheelMode::DeltaPage => LayoutVector2D::new(
                    -wheel_event.delta.x as f32 * page_height,
                    -wheel_event.delta.y as f32 * page_height,
                ),
            };
            self.pending_wheel_events.borrow_mut().insert(
                event.id,
                PendingWheelEvent {
                    webview_id,
                    point,
                    delta,
                },
            );
        }
    }

    pub fn pinch_zoom(
        &self,
        _webview_id: WebViewId,
        _pinch_zoom_delta: f32,
        _center: DevicePoint,
    ) {
        // TODO(havi-render): Forward pinch zoom.
    }

    pub fn device_pixels_per_page_pixel(
        &self,
        webview_id: WebViewId,
    ) -> Scale<f32, CSSPixel, DevicePixel> {
        let hidpi = self
            .hidpi_scale_factors
            .borrow()
            .get(&webview_id)
            .copied()
            .unwrap_or_else(Scale::identity);
        let page_zoom = self.page_zoom(webview_id);
        Scale::new(hidpi.get() * page_zoom)
    }

    pub(crate) fn shutdown_state(&self) -> ShutdownState {
        self.shutdown_state.get()
    }

    pub fn request_screenshot(
        &self,
        webview_id: WebViewId,
        rect: Option<WebViewRect>,
        callback: Box<dyn FnOnce(Result<RgbaImage, ScreenshotCaptureError>) + 'static>,
    ) {
        let device_rect = rect.map(|r| {
            r.as_device_rect(self.device_pixels_per_page_pixel(webview_id))
        });
        let request_id = self
            .screenshot_taker
            .request_screenshot(&self.screenshot_bridge, webview_id, device_rect, callback);
        self.screenshot_bridge.push_request(request_id, webview_id);
    }

    pub fn notify_input_event_handled(
        &self,
        webview_id: WebViewId,
        input_event_id: InputEventId,
        result: InputEventResult,
    ) {
        let pending = self.pending_wheel_events.borrow_mut().remove(&input_event_id);
        let Some(pending) = pending else {
            return;
        };
        if result.contains(InputEventResult::DefaultPrevented) {
            return;
        }
        if pending.webview_id != webview_id {
            return;
        }
        self.notify_scroll_default_action(webview_id, Some(pending.point), pending.delta);
    }

    pub(crate) fn notify_scroll_default_action(
        &self,
        webview_id: WebViewId,
        point: Option<Point2D<f32, CSSPixel>>,
        delta: LayoutVector2D,
    ) {
        if let Some(webview) = self.webviews.borrow().get(&webview_id) {
            (webview.notify_scroll_default_action)(point, delta);
        }
    }

    fn handle_generate_image_key(
        &self,
        webview_id: WebViewId,
        result_sender: GenericSender<ImageKey>,
    ) {
        let painter_id: PainterId = webview_id.into();
        let image_key = ImageKey::new(painter_id.into(), next_key_index());
        let _ = result_sender.send(image_key);
    }

    fn handle_generate_image_keys_for_pipeline(
        &self,
        webview_id: WebViewId,
        pipeline_id: PipelineId,
    ) {
        let painter_id: PainterId = webview_id.into();
        let image_keys = (0..pref!(image_key_batch_size))
            .map(|_| ImageKey::new(painter_id.into(), next_key_index()))
            .collect();

        let _ = self.embedder_to_constellation_sender.send(
            EmbedderToConstellationMessage::SendImageKeysForPipeline(pipeline_id, image_keys),
        );
    }

    fn handle_generate_font_keys(
        &self,
        number_of_font_keys: usize,
        number_of_font_instance_keys: usize,
        result_sender: GenericSender<(Vec<FontKey>, Vec<FontInstanceKey>)>,
        painter_id: PainterId,
    ) {
        let font_keys = (0..number_of_font_keys)
            .map(|_| FontKey::new(painter_id.into(), next_key_index()))
            .collect();
        let font_instance_keys = (0..number_of_font_instance_keys)
            .map(|_| FontInstanceKey::new(painter_id.into(), next_key_index()))
            .collect();
        let _ = result_sender.send((font_keys, font_instance_keys));
    }

    fn handle_image_updates(&self, updates: SmallVec<[paint_api::ImageUpdate; 1]>) {
        for update in updates {
            match update {
                paint_api::ImageUpdate::AddImage(key, desc, data, _is_animated) => {
                    if let paint_api::SerializableImageData::Raw(mem) = data {
                        self.image_source_store.add_image(
                            key,
                            desc.size.width as u32,
                            desc.size.height as u32,
                            mem.to_vec(),
                        );
                    }
                },
                paint_api::ImageUpdate::UpdateImage(key, desc, data, _epoch) => {
                    if let paint_api::SerializableImageData::Raw(mem) = data {
                        self.image_source_store.update_image(
                            key,
                            desc.size.width as u32,
                            desc.size.height as u32,
                            mem.to_vec(),
                        );
                    }
                },
                paint_api::ImageUpdate::UpdateImageForAnimation(key, desc) => {
                    self.image_source_store.update_frame_offset(key, desc.offset as usize);
                },
                paint_api::ImageUpdate::DeleteImage(key) => {
                    self.image_source_store.delete_image(key);
                },
            }
        }
    }
}
