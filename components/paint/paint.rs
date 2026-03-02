/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::atomic::{AtomicU32, Ordering};

use base::generic_channel::{self, GenericSender, RoutedReceiver};
use base::id::{PainterId, PipelineId, WebViewId};
use canvas_traits::webgl::{WebGLContextId, WebGLThreads};
use constellation_traits::{EmbedderToConstellationMessage, ScrollStateUpdate};
use crossbeam_channel::Sender;
use dpi::PhysicalSize;
use embedder_traits::{
    EventLoopWaker, InputEventAndId, InputEventId, InputEventResult, ScreenshotCaptureError,
    Scroll, ShutdownState, ViewportDetails, WebViewPoint, WebViewRect,
};
use euclid::{Scale, Size2D};
use image::RgbaImage;
use ipc_channel::ipc;
use log::{debug, warn};
use paint_api::rendering_context::RenderingContext;
use paint_api::{
    PaintMessage, PaintProxy, PainterGlDetails, PainterGlDetailsMap,
    WebRenderExternalImageIdManager, WebViewTrait,
};
use profile_traits::mem::{
    ProcessReports, ProfilerRegistration, Report, ReportKind,
};
use profile_traits::path;
use profile_traits::time::{self as profile_time};
use servo_config::pref;
use servo_geometry::DeviceIndependentPixel;
use style_traits::CSSPixel;
use paint_api::gl_device::swap_chain::SwapChains;
use webgl::WebGLComm;
use webgl::webgl_thread::WebGLContextBusyMap;
#[cfg(feature = "webgpu")]
use webgpu::canvas_context::WebGpuExternalImageMap;
use rustc_hash::FxHashMap;
use webrender_api::units::{DevicePixel, DevicePoint, LayoutVector2D};
use webrender_api::{ExternalScrollId, FontInstanceKey, FontKey, ImageKey};

use crate::InitialPaintState;
use crate::screenshot::ScreenshotTaker;

/// An option to control what kind of WebRender debugging is enabled while Servo is running.
#[derive(Copy, Clone)]
pub enum WebRenderDebugOption {
    Profiler,
    TextureCacheDebug,
    RenderTargetDebug,
}

/// Error type for unknown webview operations.
#[derive(Debug)]
pub struct UnknownWebView;

/// Counter for generating unique resource keys.
static NEXT_KEY_INDEX: AtomicU32 = AtomicU32::new(1);

fn next_key_index() -> u32 {
    NEXT_KEY_INDEX.fetch_add(1, Ordering::Relaxed)
}

/// [`Paint`] is Servo's rendering subsystem.
///
/// WebRender has been removed. Rendering is now handled by havi-render via Makepad.
/// This struct retains the message routing, resource key generation, WebGL/WebXR
/// infrastructure, and public API surface.
pub struct Paint {
    /// Rendering contexts registered per painter.
    rendering_contexts: HashMap<PainterId, Rc<dyn RenderingContext>>,

    /// A [`PaintProxy`] which can be used to allow other parts of Servo to communicate
    /// with this [`Paint`].
    pub(crate) paint_proxy: PaintProxy,

    /// An [`EventLoopWaker`] used to wake up the main embedder event loop.
    pub(crate) event_loop_waker: Box<dyn EventLoopWaker>,

    /// Tracks whether we are in the process of shutting down.
    shutdown_state: Rc<Cell<ShutdownState>>,

    /// The port on which we receive messages.
    paint_receiver: RoutedReceiver<PaintMessage>,

    /// The channel on which messages can be sent to the constellation.
    pub(crate) embedder_to_constellation_sender: Sender<EmbedderToConstellationMessage>,

    /// The [`WebRenderExternalImageIdManager`] used to generate new `ExternalImageId`s.
    webrender_external_image_id_manager: WebRenderExternalImageIdManager,

    /// GL display details per painter (needed for WebGL).
    pub(crate) painter_gl_details_map: PainterGlDetailsMap,

    /// WebGL context busy map.
    pub(crate) busy_webgl_contexts_map: WebGLContextBusyMap,

    /// The [`WebGLThreads`] for this renderer.
    webgl_threads: WebGLThreads,

    /// The shared [`SwapChains`] used by [`WebGLThreads`].
    pub(crate) swap_chains: SwapChains<WebGLContextId>,

    /// The channel on which messages can be sent to the time profiler.
    time_profiler_chan: profile_time::ProfilerChan,

    /// Memory profiler registration handle.
    _mem_profiler_registration: ProfilerRegistration,

    /// Screenshot taker.
    screenshot_taker: ScreenshotTaker,

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
    pending_wheel_events: RefCell<FxHashMap<InputEventId, PendingWheelEvent>>,

    /// Root scroll offset per webview. Updated when wheel events are processed
    /// and sent to layout via `SetScrollStates`.
    root_scroll_offsets: RefCell<HashMap<WebViewId, LayoutVector2D>>,

    /// Mapping of webview to its root pipeline, updated via `SetFrameTreeForWebView`.
    webview_pipelines: RefCell<HashMap<WebViewId, PipelineId>>,
}

/// A wheel event stored while waiting for the script thread to report whether
/// `preventDefault()` was called.
struct PendingWheelEvent {
    delta: embedder_traits::WheelDelta,
}

impl Paint {
    pub fn new(state: InitialPaintState) -> Rc<RefCell<Self>> {
        let registration = state.mem_profiler_chan.prepare_memory_reporting(
            "paint".into(),
            state.paint_proxy.clone(),
            PaintMessage::CollectMemoryReport,
        );

        let webrender_external_image_id_manager = WebRenderExternalImageIdManager::default();
        let painter_gl_details_map = PainterGlDetailsMap::default();
        let WebGLComm {
            webgl_threads,
            swap_chains,
            busy_webgl_context_map,
            #[cfg(feature = "webxr")]
            webxr_layer_grand_manager,
        } = WebGLComm::new(
            state.paint_proxy.cross_process_paint_api.clone(),
            webrender_external_image_id_manager.clone(),
            painter_gl_details_map.clone(),
        );

        #[cfg(feature = "webxr")]
        let webxr_main_thread = {
            use servo_config::pref;

            let mut webxr_main_thread = webxr::MainThreadRegistry::new(
                state.event_loop_waker.clone(),
                webxr_layer_grand_manager,
            )
            .expect("Failed to create WebXR device registry");
            if pref!(dom_webxr_enabled) {
                state.webxr_registry.register(&mut webxr_main_thread);
            }
            webxr_main_thread
        };

        Rc::new(RefCell::new(Paint {
            rendering_contexts: Default::default(),
            paint_proxy: state.paint_proxy,
            event_loop_waker: state.event_loop_waker,
            shutdown_state: state.shutdown_state,
            paint_receiver: state.receiver,
            embedder_to_constellation_sender: state.embedder_to_constellation_sender.clone(),
            webrender_external_image_id_manager,
            webgl_threads,
            swap_chains,
            time_profiler_chan: state.time_profiler_chan,
            _mem_profiler_registration: registration,
            painter_gl_details_map,
            busy_webgl_contexts_map: busy_webgl_context_map,
            screenshot_taker: Default::default(),
            page_zooms: Default::default(),
            hidpi_scale_factors: Default::default(),
            #[cfg(feature = "webxr")]
            webxr_main_thread: RefCell::new(webxr_main_thread),
            #[cfg(feature = "webgpu")]
            webgpu_image_map: Default::default(),
            pending_wheel_events: Default::default(),
            root_scroll_offsets: Default::default(),
            webview_pipelines: Default::default(),
        }))
    }

    pub fn register_rendering_context(
        &mut self,
        rendering_context: Rc<dyn RenderingContext>,
    ) -> PainterId {
        // Check if this rendering context is already registered.
        if let Some(painter_id) = self
            .rendering_contexts
            .iter()
            .find_map(|(id, rc)| Rc::ptr_eq(rc, &rendering_context).then_some(*id))
        {
            return painter_id;
        }

        let painter_id = PainterId::next();

        if let Some(display_info) = rendering_context.gl_display_info() {
            let painter_gl_details = PainterGlDetails { display_info };
            self.painter_gl_details_map
                .insert(painter_id, painter_gl_details);
        } else {
            warn!(
                "RenderingContext for painter {:?} does not provide gl_display_info; WebGL disabled",
                painter_id
            );
        }

        self.rendering_contexts
            .insert(painter_id, rendering_context);
        painter_id
    }

    pub fn painter_id(&self) -> PainterId {
        *self
            .rendering_contexts
            .keys()
            .next()
            .expect("No rendering contexts registered")
    }

    pub fn rendering_context_size(&self, painter_id: PainterId) -> Size2D<u32, DevicePixel> {
        self.rendering_contexts[&painter_id].size2d()
    }

    pub fn webgl_threads(&self) -> WebGLThreads {
        self.webgl_threads.clone()
    }

    pub fn webrender_external_image_id_manager(&self) -> WebRenderExternalImageIdManager {
        self.webrender_external_image_id_manager.clone()
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

    pub fn webviews_needing_repaint(&self) -> Vec<WebViewId> {
        // TODO(havi-render): Repaint tracking via Makepad redraw signals.
        Vec::new()
    }

    pub fn finish_shutting_down(&self) {
        while self.paint_receiver.try_recv().is_ok() {}

        let (webgl_exit_sender, webgl_exit_receiver) =
            generic_channel::channel().expect("Failed to create IPC channel!");
        if !self
            .webgl_threads
            .exit(webgl_exit_sender)
            .is_ok_and(|_| webgl_exit_receiver.recv().is_ok())
        {
            warn!("Could not exit WebGLThread.");
        }

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
            PaintMessage::ScrollNodeByDelta(..) => {
                // TODO(havi-render): Scroll via havi-render.
            },
            PaintMessage::ScrollViewportByDelta(..) => {
                // TODO(havi-render): Scroll via havi-render.
            },
            PaintMessage::UpdateEpoch { .. } => {},
            PaintMessage::GenerateFrame(..) => {
                // TODO(havi-render): Trigger Makepad redraw.
            },
            PaintMessage::GenerateImageKey(webview_id, result_sender) => {
                self.handle_generate_image_key(webview_id, result_sender);
            },
            PaintMessage::GenerateImageKeysForPipeline(webview_id, pipeline_id) => {
                self.handle_generate_image_keys_for_pipeline(webview_id, pipeline_id);
            },
            PaintMessage::UpdateImages(..) => {
                // TODO(havi-render): Forward image updates to Makepad texture cache.
            },
            PaintMessage::DelayNewFrameForCanvas(..) => {},
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
            PaintMessage::ScreenshotReadinessReponse(..) => {
                // TODO(havi-render): Wire screenshot via Makepad.
            },
            PaintMessage::SendLCPCandidate(..) => {},
            PaintMessage::EnableLCPCalculation(..) => {},
        }
    }

    pub fn remove_webview(&mut self, _webview_id: WebViewId) {
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
        _webview: Box<dyn WebViewTrait>,
        _viewport_details: ViewportDetails,
    ) {
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

    pub fn resize_rendering_context(
        &self,
        _webview_id: WebViewId,
        _new_size: PhysicalSize<u32>,
    ) {
        // TODO(havi-render): Resize handled by Makepad.
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

    /// Render. TODO(havi-render): This is now a no-op; Makepad draws directly.
    pub fn render(&self, _webview_id: WebViewId) {}

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
    pub fn perform_updates(&self) -> bool {
        if self.shutdown_state() == ShutdownState::FinishedShuttingDown {
            return false;
        }

        #[cfg(feature = "webxr")]
        self.webxr_main_thread.borrow_mut().run_one_frame();

        self.shutdown_state() != ShutdownState::FinishedShuttingDown
    }

    pub fn toggle_webrender_debug(&self, _option: WebRenderDebugOption) {}

    pub fn capture_webrender(&self, _webview_id: WebViewId) {}

    pub fn notify_input_event(&self, _webview_id: WebViewId, event: InputEventAndId) {
        if let embedder_traits::InputEvent::Wheel(ref wheel_event) = event.event {
            self.pending_wheel_events.borrow_mut().insert(
                event.id,
                PendingWheelEvent {
                    delta: wheel_event.delta,
                },
            );
        }
    }

    pub fn notify_scroll_event(
        &self,
        webview_id: WebViewId,
        scroll: Scroll,
        _point: WebViewPoint,
    ) {
        let dpp = self.device_pixels_per_page_pixel(webview_id);
        let layout_delta = match scroll {
            Scroll::Delta(delta) => {
                let device_delta = delta.as_device_vector(dpp);
                // Convert device pixels to layout (CSS) pixels.
                LayoutVector2D::new(device_delta.x / dpp.get(), device_delta.y / dpp.get())
            },
            // Start/End require content size which we don't track yet.
            Scroll::Start | Scroll::End => return,
        };
        self.apply_scroll_delta(webview_id, layout_delta);
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
        self.screenshot_taker
            .request_screenshot(webview_id, device_rect, callback);
    }

    pub fn notify_input_event_handled(
        &self,
        webview_id: WebViewId,
        input_event_id: InputEventId,
        result: InputEventResult,
    ) {
        let Some(wheel_event) = self.pending_wheel_events.borrow_mut().remove(&input_event_id)
        else {
            return;
        };

        // If script called preventDefault(), don't perform default scroll.
        if result.contains(InputEventResult::DefaultPrevented) {
            return;
        }

        // Default scroll action: apply the inverse of the wheel delta as a
        // scroll offset change. Positive wheel delta reveals content above (scroll up),
        // so we negate to get the offset change.
        use embedder_traits::WheelMode;
        let line_height = 16.0; // CSS px
        let page_height = 800.0; // CSS px fallback
        let (dx, dy) = match wheel_event.delta.mode {
            WheelMode::DeltaPixel => {
                let dpp = self.device_pixels_per_page_pixel(webview_id);
                (
                    -wheel_event.delta.x as f32 / dpp.get(),
                    -wheel_event.delta.y as f32 / dpp.get(),
                )
            },
            WheelMode::DeltaLine => (
                -wheel_event.delta.x as f32 * line_height,
                -wheel_event.delta.y as f32 * line_height,
            ),
            WheelMode::DeltaPage => (
                -wheel_event.delta.x as f32 * page_height,
                -wheel_event.delta.y as f32 * page_height,
            ),
        };
        self.apply_scroll_delta(webview_id, LayoutVector2D::new(dx, dy));
    }

    /// Apply a scroll delta to the root scroll node of the given webview and
    /// send the updated offset to layout via `SetScrollStates`.
    fn apply_scroll_delta(&self, webview_id: WebViewId, delta: LayoutVector2D) {
        let Some(&pipeline_id) = self.webview_pipelines.borrow().get(&webview_id) else {
            warn!("apply_scroll_delta: no pipeline for webview {:?}", webview_id);
            return;
        };

        let mut offsets = self.root_scroll_offsets.borrow_mut();
        let offset = offsets.entry(webview_id).or_insert_with(LayoutVector2D::zero);

        // Apply delta. Clamp to >= 0 (layout will clamp the upper bound when
        // it knows content size). Negative offset is meaningless.
        offset.x = (offset.x + delta.x).max(0.0);
        offset.y = (offset.y + delta.y).max(0.0);

        let root_scroll_id = ExternalScrollId(0, pipeline_id.into());
        let mut scroll_offsets = FxHashMap::default();
        scroll_offsets.insert(root_scroll_id, *offset);

        let _ = self.embedder_to_constellation_sender.send(
            EmbedderToConstellationMessage::SetScrollStates(
                pipeline_id,
                ScrollStateUpdate {
                    scrolled_node: root_scroll_id,
                    offsets: scroll_offsets,
                },
            ),
        );
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
}
