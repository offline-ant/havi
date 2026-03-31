/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! The interface to the `paint` crate, which helps to break dependency cycles.

use std::collections::HashMap;
use std::fmt::{Debug, Error, Formatter};

use base::Epoch;
use base::id::{PainterId, PipelineId, WebViewId};
use crossbeam_channel::Sender;
use embedder_traits::{AnimationState, EventLoopWaker};
use euclid::{Rect, Scale, Size2D};
use log::warn;
use malloc_size_of_derive::MallocSizeOf;
use parking_lot::RwLock;
use rustc_hash::FxHashMap;
use smallvec::SmallVec;
use strum::IntoStaticStr;
use style_traits::CSSPixel;
use webrender_api::FontVariation;

use std::sync::Arc;

use base::generic_channel::{
    self, GenericCallback, GenericSender, GenericSharedMemory,
};
use bitflags::bitflags;
use euclid::default::Size2D as UntypedSize2D;
use profile_traits::mem::{OpaqueSender, ReportsChan};
use serde::{Deserialize, Serialize};
pub use webrender_api::ExternalImageSource;
use webrender_api::units::{DevicePixel, LayoutVector2D, TexelRect};
use webrender_api::{
    ExternalImage, ExternalImageData, ExternalImageHandler, ExternalImageId,
    FontInstanceFlags, FontInstanceKey, FontKey, ImageData, ImageDescriptor, ImageKey,
    NativeFontHandle,
};

use crate::paint::largest_contentful_paint_candidate::LCPCandidate;
use crate::paint::viewport_description::ViewportDescription;

/// Sends messages to `Paint`.
#[derive(Clone)]
pub struct PaintProxy {
    pub sender: Sender<Result<PaintMessage, ipc_channel::IpcError>>,
    /// Access to [`Self::sender`] that is possible to send across an IPC
    /// channel. These messages are routed via the router thread to
    /// [`Self::sender`].
    pub cross_process_paint_api: CrossProcessPaintApi,
    pub event_loop_waker: Box<dyn EventLoopWaker>,
}

impl OpaqueSender<PaintMessage> for PaintProxy {
    fn send(&self, message: PaintMessage) {
        PaintProxy::send(self, message)
    }
}

impl PaintProxy {
    pub fn send(&self, msg: PaintMessage) {
        self.route_msg(Ok(msg))
    }

    /// Helper method to route a deserialized IPC message to the receiver.
    ///
    /// This method is a temporary solution, and will be removed when migrating
    /// to `GenericChannel`.
    pub fn route_msg(&self, msg: Result<PaintMessage, ipc_channel::IpcError>) {
        if let Err(err) = self.sender.send(msg) {
            warn!("Failed to send response ({:?}).", err);
        }
        self.event_loop_waker.wake();
    }
}

/// Messages from (or via) the constellation thread to `Paint`.
#[derive(Deserialize, IntoStaticStr, Serialize)]
pub enum PaintMessage {
    /// Alerts `Paint` that the given pipeline has changed whether it is running animations.
    ChangeRunningAnimationsState(WebViewId, PipelineId, AnimationState),
    /// Updates the frame tree for the given webview.
    SetFrameTreeForWebView(WebViewId, SendableFrameTree),
    /// Set whether to use less resources by stopping animations.
    SetThrottled(WebViewId, PipelineId, bool),
    /// Script or the Constellation is notifying the renderer that a Pipeline has finished
    /// shutting down. The renderer will not discard the Pipeline until both report that
    /// they have fully shut it down, to avoid recreating it due to any subsequent
    /// messages.
    PipelineExited(WebViewId, PipelineId, PipelineExitSource),
    /// Scroll the WebView's viewport by the given delta. This will also do panning
    /// in the pinch zoom viewport if possible and the remaining delta will be used
    /// to scroll the root layer.
    ScrollViewportByDelta(WebViewId, LayoutVector2D),
    /// Update the rendering epoch of the given `Pipeline`.
    UpdateEpoch {
        /// The [`WebViewId`] that this display list belongs to.
        webview_id: WebViewId,
        /// The [`PipelineId`] of the `Pipeline` to update.
        pipeline_id: PipelineId,
        /// The new [`Epoch`] value.
        epoch: Epoch,
    },
    /// Ask the renderer to generate a frame for the current set of display lists
    /// from the given `PainterId`s that have been sent to the renderer.
    GenerateFrame(Vec<PainterId>),
    /// Create a new image key. The result will be returned via the
    /// provided channel sender.
    GenerateImageKey(WebViewId, GenericSender<ImageKey>),
    /// The same as the above but it will be forwarded to the pipeline instead
    /// of send via a channel.
    GenerateImageKeysForPipeline(WebViewId, PipelineId),
    /// Perform a resource update operation.
    UpdateImages(PainterId, SmallVec<[ImageUpdate; 1]>),
    /// Pause all pipeline display list processing for the given pipeline until the
    /// following image updates have been received. This is used to ensure that canvas
    /// elements have had a chance to update their rendering and send the image update to
    /// the renderer before their associated display list is actually displayed.
    DelayNewFrameForCanvas(WebViewId, PipelineId, Epoch, Vec<ImageKey>),

    /// Generate a new batch of font keys which can be used to allocate
    /// keys asynchronously.
    GenerateFontKeys(
        usize,
        usize,
        GenericSender<(Vec<FontKey>, Vec<FontInstanceKey>)>,
        PainterId,
    ),
    /// Add a font with the given data and font key.
    AddFont(PainterId, FontKey, Arc<GenericSharedMemory>, u32),
    /// Add a system font with the given font key and handle.
    AddSystemFont(PainterId, FontKey, NativeFontHandle),
    /// Add an instance of a font with the given instance key.
    AddFontInstance(
        PainterId,
        FontInstanceKey,
        FontKey,
        f32,
        FontInstanceFlags,
        Vec<FontVariation>,
    ),
    /// Remove the given font resources from the render backend.
    RemoveFonts(PainterId, Vec<FontKey>, Vec<FontInstanceKey>),
    /// Measure the current memory usage associated with `Paint`.
    /// The report must be sent on the provided channel once it's complete.
    CollectMemoryReport(ReportsChan),
    /// A top-level frame has parsed a viewport metatag and is sending the new constraints.
    Viewport(WebViewId, ViewportDescription),
    /// Let `Paint` know that the given WebView is ready to have a screenshot taken
    /// after the given pipeline's epochs have been rendered.
    ScreenshotReadinessReponse(WebViewId, FxHashMap<PipelineId, Epoch>),
    /// The candidate of largest-contentful-paint
    SendLCPCandidate(LCPCandidate, WebViewId, PipelineId, Epoch),
    /// Enable LCP calculation for the given WebView.
    EnableLCPCalculation(WebViewId),
}

impl Debug for PaintMessage {
    fn fmt(&self, formatter: &mut Formatter) -> Result<(), Error> {
        let string: &'static str = self.into();
        write!(formatter, "{string}")
    }
}

#[derive(Deserialize, Serialize)]
pub struct SendableFrameTree {
    pub pipeline: CompositionPipeline,
    pub children: Vec<SendableFrameTree>,
}

/// The subset of the pipeline that is needed for layer composition.
#[derive(Clone, Deserialize, Serialize)]
pub struct CompositionPipeline {
    pub id: PipelineId,
    pub webview_id: WebViewId,
}

/// A mechanism to send messages from ScriptThread to the parent process paint subsystem.
#[derive(Clone, Deserialize, MallocSizeOf, Serialize)]
pub struct CrossProcessPaintApi(GenericCallback<PaintMessage>);

impl CrossProcessPaintApi {
    /// Create a new [`CrossProcessPaintApi`] struct.
    pub fn new(callback: GenericCallback<PaintMessage>) -> Self {
        CrossProcessPaintApi(callback)
    }

    /// Create a new [`CrossProcessPaintApi`] struct that does not have a listener on the other
    /// end to use for unit testing.
    pub fn dummy() -> Self {
        Self::dummy_with_callback(None)
    }

    /// Create a new [`CrossProcessPaintApi`] struct for unit testing with an optional callback
    /// that can respond to `PaintMessage`s.
    pub fn dummy_with_callback(
        callback: Option<Box<dyn Fn(PaintMessage) + Send + 'static>>,
    ) -> Self {
        let callback = GenericCallback::new(move |msg| {
            if let Some(ref handler) = callback {
                if let Ok(paint_message) = msg {
                    handler(paint_message);
                }
            }
        })
        .unwrap();
        Self(callback)
    }

    /// Scroll the WebView's viewport by the given delta. This will also do panning
    /// in the pinch zoom viewport if possible and the remaining delta will be used
    /// to scroll the root layer.
    ///
    /// Note the value provided here is in `DeviceIndependentPixels` and will first be
    /// converted to `DevicePixels` by the renderer.
    pub fn scroll_viewport_by_delta(&self, webview_id: WebViewId, delta: LayoutVector2D) {
        if let Err(error) = self
            .0
            .send(PaintMessage::ScrollViewportByDelta(webview_id, delta))
        {
            warn!("Error scroll viewport: {error}");
        }
    }

    pub fn delay_new_frame_for_canvas(
        &self,
        webview_id: WebViewId,
        pipeline_id: PipelineId,
        canvas_epoch: Epoch,
        image_keys: Vec<ImageKey>,
    ) {
        if let Err(error) = self.0.send(PaintMessage::DelayNewFrameForCanvas(
            webview_id,
            pipeline_id,
            canvas_epoch,
            image_keys,
        )) {
            warn!("Error delaying frames for canvas image updates {error:?}");
        }
    }

    /// Inform the renderer that the rendering epoch has advanced. This typically happens after
    /// a new display list is sent and/or canvas and animated images are updated.
    pub fn update_epoch(&self, webview_id: WebViewId, pipeline_id: PipelineId, epoch: Epoch) {
        if let Err(error) = self.0.send(PaintMessage::UpdateEpoch {
            webview_id,
            pipeline_id,
            epoch,
        }) {
            warn!("Error updating epoch for pipeline: {error:?}");
        }
    }

    /// Send the largest contentful paint candidate to `Paint`.
    pub fn send_lcp_candidate(
        &self,
        lcp_candidate: LCPCandidate,
        webview_id: WebViewId,
        pipeline_id: PipelineId,
        epoch: Epoch,
    ) {
        if let Err(error) = self.0.send(PaintMessage::SendLCPCandidate(
            lcp_candidate,
            webview_id,
            pipeline_id,
            epoch,
        )) {
            warn!("Error sending LCPCandidate: {error}");
        }
    }

    /// Ask the Servo renderer to generate a new frame after having new display lists.
    pub fn generate_frame(&self, painter_ids: Vec<PainterId>) {
        if let Err(error) = self.0.send(PaintMessage::GenerateFrame(painter_ids)) {
            warn!("Error generating frame: {error}");
        }
    }

    /// Create a new image key. Blocks until the key is available.
    pub fn generate_image_key_blocking(&self, webview_id: WebViewId) -> Option<ImageKey> {
        let (sender, receiver) = generic_channel::channel().unwrap();
        self.0
            .send(PaintMessage::GenerateImageKey(webview_id, sender))
            .ok()?;
        receiver.recv().ok()
    }

    /// Sends a message to `Paint` for creating new image keys.
    /// `Paint` will then send a batch of keys over the constellation to the script_thread
    /// and the appropriate pipeline.
    pub fn generate_image_key_async(&self, webview_id: WebViewId, pipeline_id: PipelineId) {
        if let Err(e) = self.0.send(PaintMessage::GenerateImageKeysForPipeline(
            webview_id,
            pipeline_id,
        )) {
            warn!("Could not send image keys to Paint {}", e);
        }
    }

    pub fn add_image(
        &self,
        key: ImageKey,
        descriptor: ImageDescriptor,
        data: SerializableImageData,
        is_animated_image: bool,
    ) {
        self.update_images(
            key.into(),
            [ImageUpdate::AddImage(
                key,
                descriptor,
                data,
                is_animated_image,
            )]
            .into(),
        );
    }

    pub fn update_image(
        &self,
        key: ImageKey,
        descriptor: ImageDescriptor,
        data: SerializableImageData,
        epoch: Option<Epoch>,
    ) {
        self.update_images(
            key.into(),
            [ImageUpdate::UpdateImage(key, descriptor, data, epoch)].into(),
        );
    }

    pub fn delete_image(&self, key: ImageKey) {
        self.update_images(key.into(), [ImageUpdate::DeleteImage(key)].into());
    }

    /// Perform an image resource update operation.
    pub fn update_images(&self, painter_id: PainterId, updates: SmallVec<[ImageUpdate; 1]>) {
        if let Err(e) = self.0.send(PaintMessage::UpdateImages(painter_id, updates)) {
            warn!("error sending image updates: {}", e);
        }
    }

    pub fn remove_unused_font_resources(
        &self,
        painter_id: PainterId,
        keys: Vec<FontKey>,
        instance_keys: Vec<FontInstanceKey>,
    ) {
        if keys.is_empty() && instance_keys.is_empty() {
            return;
        }
        let _ = self
            .0
            .send(PaintMessage::RemoveFonts(painter_id, keys, instance_keys));
    }

    pub fn add_font_instance(
        &self,
        font_instance_key: FontInstanceKey,
        font_key: FontKey,
        size: f32,
        flags: FontInstanceFlags,
        variations: Vec<FontVariation>,
    ) {
        let _x = self.0.send(PaintMessage::AddFontInstance(
            font_key.into(),
            font_instance_key,
            font_key,
            size,
            flags,
            variations,
        ));
    }

    pub fn add_font(&self, font_key: FontKey, data: Arc<GenericSharedMemory>, index: u32) {
        let _ = self.0.send(PaintMessage::AddFont(
            font_key.into(),
            font_key,
            data,
            index,
        ));
    }

    pub fn add_system_font(&self, font_key: FontKey, handle: NativeFontHandle) {
        let _ = self.0.send(PaintMessage::AddSystemFont(
            font_key.into(),
            font_key,
            handle,
        ));
    }

    pub fn fetch_font_keys(
        &self,
        number_of_font_keys: usize,
        number_of_font_instance_keys: usize,
        painter_id: PainterId,
    ) -> (Vec<FontKey>, Vec<FontInstanceKey>) {
        let (sender, receiver) = generic_channel::channel().expect("Could not create IPC channel");
        let _ = self.0.send(PaintMessage::GenerateFontKeys(
            number_of_font_keys,
            number_of_font_instance_keys,
            sender,
            painter_id,
        ));
        receiver.recv().unwrap()
    }

    pub fn viewport(&self, webview_id: WebViewId, description: ViewportDescription) {
        let _ = self.0.send(PaintMessage::Viewport(webview_id, description));
    }

    pub fn pipeline_exited(
        &self,
        webview_id: WebViewId,
        pipeline_id: PipelineId,
        source: PipelineExitSource,
    ) {
        let _ = self.0.send(PaintMessage::PipelineExited(
            webview_id,
            pipeline_id,
            source,
        ));
    }
}

/// This trait bridges external image producers with the render backend's
/// `ExternalImageHandler` API.
///
/// It is used to notify lock/unlock messages and provide the image data needed
/// by the render backend.
pub trait ExternalImageProvider {
    fn lock(&mut self, id: u64) -> (ExternalImageSource<'_>, UntypedSize2D<i32>);
    fn unlock(&mut self, id: u64);
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ExternalImageProducerType {
    Media,
    WebGpu,
    Paint,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ExternalImageBackingType {
    NativeTexture,
    Buffer,
}

/// Paint external-image registration metadata.
///
/// This stays scoped to paint-owned producers. It is not the retained browser
/// renderer resource model.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ExternalImageHandlerType {
    pub producer: ExternalImageProducerType,
    pub backing: ExternalImageBackingType,
}

impl ExternalImageHandlerType {
    pub const MEDIA_NATIVE_TEXTURE: Self = Self {
        producer: ExternalImageProducerType::Media,
        backing: ExternalImageBackingType::NativeTexture,
    };
    pub const MEDIA_BUFFER: Self = Self {
        producer: ExternalImageProducerType::Media,
        backing: ExternalImageBackingType::Buffer,
    };
    pub const WEBGPU_NATIVE_TEXTURE: Self = Self {
        producer: ExternalImageProducerType::WebGpu,
        backing: ExternalImageBackingType::NativeTexture,
    };
    pub const WEBGPU_BUFFER: Self = Self {
        producer: ExternalImageProducerType::WebGpu,
        backing: ExternalImageBackingType::Buffer,
    };
    pub const PAINT_NATIVE_TEXTURE: Self = Self {
        producer: ExternalImageProducerType::Paint,
        backing: ExternalImageBackingType::NativeTexture,
    };
    pub const PAINT_BUFFER: Self = Self {
        producer: ExternalImageProducerType::Paint,
        backing: ExternalImageBackingType::Buffer,
    };
}

/// Registry of external images shared among all paint-owned external image
/// consumers.
/// It ensures that external image identifiers are unique.
#[derive(Default)]
struct ExternalImageIdRegistryInner {
    /// Map of all generated external images.
    external_images: FxHashMap<ExternalImageId, ExternalImageHandlerType>,
    /// Id generator for the next external image identifier.
    next_image_id: u64,
}

#[derive(Default, Clone)]
pub struct ExternalImageIdRegistry(Arc<RwLock<ExternalImageIdRegistryInner>>);

impl ExternalImageIdRegistry {
    pub fn next_id(&mut self, handler_type: ExternalImageHandlerType) -> ExternalImageId {
        let mut inner = self.0.write();
        inner.next_image_id += 1;
        let key = ExternalImageId(inner.next_image_id);
        inner.external_images.insert(key, handler_type);
        key
    }

    pub fn remove(&mut self, key: &ExternalImageId) {
        self.0.write().external_images.remove(key);
    }

    pub fn get(&self, key: &ExternalImageId) -> Option<ExternalImageHandlerType> {
        self.0.read().external_images.get(key).cloned()
    }
}

/// External image handler implementation.
pub struct ExternalImageHandlers {
    handlers: HashMap<ExternalImageProducerType, Box<dyn ExternalImageProvider>>,
    /// An [`ExternalImageIdRegistry`] responsible for creating new [`ExternalImageId`]s.
    /// This is shared with the WebGPU and hardware-accelerated media threads and
    /// all other instances of [`ExternalImageHandlers`] in the process.
    id_manager: ExternalImageIdRegistry,
}

impl ExternalImageHandlers {
    pub fn new(id_manager: ExternalImageIdRegistry) -> Self {
        Self {
            handlers: HashMap::new(),
            id_manager,
        }
    }

    pub fn id_manager(&self) -> ExternalImageIdRegistry {
        self.id_manager.clone()
    }

    pub fn set_handler(
        &mut self,
        handler: Box<dyn ExternalImageProvider>,
        handler_type: ExternalImageHandlerType,
    ) {
        self.handlers.insert(handler_type.producer, handler);
    }
}

impl ExternalImageHandler for ExternalImageHandlers {
    /// Lock the external image so the render backend can read its content.
    /// The producer should not change the image content until `unlock()` is called.
    fn lock(
        &mut self,
        key: ExternalImageId,
        _channel_index: u8,
        _is_composited: bool,
    ) -> ExternalImage<'_> {
        let handler_type = self
            .id_manager()
            .get(&key)
            .expect("Tried to get unknown external image");
        let (source, size) = self
            .handlers
            .get_mut(&handler_type.producer)
            .expect("Tried to lock external image without registered producer handler")
            .lock(key.0);
        let source = match handler_type.backing {
            ExternalImageBackingType::NativeTexture => match source {
                ExternalImageSource::NativeTexture(texture_id) => {
                    ExternalImageSource::NativeTexture(texture_id)
                }
                _ => panic!("external image registered as native texture returned buffer data"),
            },
            ExternalImageBackingType::Buffer => source,
        };
        ExternalImage {
            uv: TexelRect::new(0.0, size.height as f32, size.width as f32, 0.0),
            source,
        }
    }

    /// Unlock the external image after the render backend is done reading it.
    fn unlock(&mut self, key: ExternalImageId, _channel_index: u8) {
        let handler_type = self
            .id_manager()
            .get(&key)
            .expect("Tried to get unknown external image");
        self.handlers
            .get_mut(&handler_type.producer)
            .expect("Tried to unlock external image without registered producer handler")
            .unlock(key.0);
    }
}

#[derive(Deserialize, Serialize)]
/// Serializable image updates that must be performed by the render backend.
pub enum ImageUpdate {
    /// Register a new image.
    AddImage(
        ImageKey,
        ImageDescriptor,
        SerializableImageData,
        bool, /* is_animated_image */
    ),
    /// Delete a previously registered image registration.
    DeleteImage(ImageKey),
    /// Update an existing image registration.
    UpdateImage(
        ImageKey,
        ImageDescriptor,
        SerializableImageData,
        Option<Epoch>,
    ),
    /// Update an [`ImageDescriptor`] for an existing image. This is used primarily
    /// to modify the data offset for image animations.
    UpdateImageForAnimation(ImageKey, ImageDescriptor),
}

impl Debug for ImageUpdate {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::AddImage(image_key, image_desc, _, is_animated_image) => f
                .debug_tuple("AddImage")
                .field(image_key)
                .field(image_desc)
                .field(is_animated_image)
                .finish(),
            Self::DeleteImage(image_key) => f.debug_tuple("DeleteImage").field(image_key).finish(),
            Self::UpdateImage(image_key, image_desc, _, epoch) => f
                .debug_tuple("UpdateImage")
                .field(image_key)
                .field(image_desc)
                .field(epoch)
                .finish(),
            Self::UpdateImageForAnimation(image_key, image_desc) => f
                .debug_tuple("UpdateAnimation")
                .field(image_key)
                .field(image_desc)
                .finish(),
        }
    }
}

#[derive(Debug, Deserialize, Serialize)]
/// Serialized `ImageData`.
pub enum SerializableImageData {
    /// A simple series of bytes, provided by the embedding and owned by the render backend.
    /// The format is stored out-of-band, currently in ImageDescriptor.
    Raw(GenericSharedMemory),
    /// An image owned by the embedding, and referenced by the render backend. This may
    /// take the form of a texture or a heap-allocated buffer.
    External(ExternalImageData),
}

impl From<SerializableImageData> for ImageData {
    fn from(value: SerializableImageData) -> Self {
        match value {
            SerializableImageData::Raw(shared_memory) => ImageData::new(shared_memory.to_vec()),
            SerializableImageData::External(image) => ImageData::External(image),
        }
    }
}

/// What entity is reporting that a `Pipeline` has exited. Only when all have
/// done this will the renderer discard its details.
#[derive(Clone, Copy, Default, Deserialize, PartialEq, Serialize)]
pub struct PipelineExitSource(u8);

bitflags! {
    impl PipelineExitSource: u8 {
        const Script = 1 << 0;
        const Constellation = 1 << 1;
    }
}

/// A [`PinchZoomInfos`] for a root [`Pipeline`] of an [`WebView`]. For any [`Pipeline`]
/// that is not a root, it should follow the viewport description of its pipeline since
/// pinch-zoom and resizing due to overlay UIs are not applicable there.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
pub struct PinchZoomInfos {
    /// The zoom factor (or pinch-zoom).
    pub zoom_factor: Scale<f32, DevicePixel, DevicePixel>,

    /// The size relative to layout viewport.
    pub rect: Rect<f32, CSSPixel>,
}

impl PinchZoomInfos {
    /// New initial [`PinchZoomInfos`] without any pinch-zoom or resizing from a viewport size
    /// for a nested pipeline or newly initialized root pipeline.
    pub fn new_from_viewport_size(size: Size2D<f32, CSSPixel>) -> Self {
        Self {
            zoom_factor: Scale::identity(),
            rect: Rect::from_size(size),
        }
    }
}

// ---------------------------------------------------------------------------
// Shared image source store — bridges Paint-layer image updates into the
// retained renderer resource model.
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
struct SharedImageSourceEntry {
    data: Arc<Vec<u8>>,
    width: u32,
    height: u32,
    offset: usize,
    revision: u64,
}

/// Thread-safe source store shared between Paint and the Makepad renderer path.
///
/// This stores stable image-source bytes keyed by Servo image key. The render
/// path snapshots these records and syncs them into the renderer-owned browser
/// resource registry. It is not an image-override side channel.
#[derive(Clone, Default)]
pub struct SharedImageSourceStore(Arc<RwLock<SharedImageSourceStoreInner>>);

#[derive(Default)]
struct SharedImageSourceStoreInner {
    images: HashMap<ImageKey, SharedImageSourceEntry>,
}

impl SharedImageSourceStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_image(&self, key: ImageKey, width: u32, height: u32, data: Vec<u8>) {
        self.0.write().images.insert(key, SharedImageSourceEntry {
            data: Arc::new(data),
            width,
            height,
            offset: 0,
            revision: 1,
        });
    }

    pub fn update_image(&self, key: ImageKey, width: u32, height: u32, data: Vec<u8>) {
        let data = Arc::new(data);
        let mut inner = self.0.write();
        let entry = inner.images.entry(key).or_insert_with(|| SharedImageSourceEntry {
            data: data.clone(),
            width,
            height,
            offset: 0,
            revision: 0,
        });
        entry.data = data;
        entry.width = width;
        entry.height = height;
        entry.offset = 0;
        entry.revision = entry.revision.wrapping_add(1).max(1);
    }

    pub fn update_frame_offset(&self, key: ImageKey, offset: usize) {
        if let Some(entry) = self.0.write().images.get_mut(&key) {
            entry.offset = offset;
            entry.revision = entry.revision.wrapping_add(1).max(1);
        }
    }

    pub fn delete_image(&self, key: ImageKey) {
        self.0.write().images.remove(&key);
    }

    pub fn snapshot(&self) -> havi_types::SharedImageSourceMap {
        let inner = self.0.read();
        inner
            .images
            .iter()
            .map(|(key, entry)| {
                (
                    havi_types::FragmentImageKey::from((key.0.0, key.1)),
                    havi_types::SharedImageSource {
                        data: entry.data.clone(),
                        offset: entry.offset,
                        width: entry.width,
                        height: entry.height,
                        revision: entry.revision,
                    },
                )
            })
            .collect()
    }
}
