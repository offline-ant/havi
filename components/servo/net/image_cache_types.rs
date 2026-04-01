/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

use std::sync::Arc;

use base::id::PipelineId;
use log::debug;
use malloc_size_of::MallocSizeOfOps;
use malloc_size_of_derive::MallocSizeOf;
use crate::pixels::{CorsStatus, ImageMetadata, RasterImage};
use profile_traits::mem::Report;
use serde::{Deserialize, Serialize};
use servo_url::{ImmutableOrigin, BrowserUrl};
use webrender_api::ImageKey;

use crate::net::FetchResponseMsg;
use crate::net::request::CorsSettings;

// ======================================================================
// Aux structs and enums.
// ======================================================================

pub type VectorImageId = PendingImageId;

// Represents either a decode-backed raster image with CPU-side bytes available
// or a vector image for which natural dimensions and original SVG bytes are
// cached for native SVG consumers.
#[derive(Clone, Debug, MallocSizeOf)]
pub enum Image {
    Raster(#[conditional_malloc_size_of] Arc<RasterImage>),
    Vector(VectorImage),
}

#[derive(Clone, Debug, Deserialize, MallocSizeOf, Serialize)]
pub struct VectorImage {
    pub id: VectorImageId,
    pub metadata: ImageMetadata,
    pub cors_status: CorsStatus,
}

impl Image {
    pub fn metadata(&self) -> ImageMetadata {
        match self {
            Image::Vector(image) => image.metadata,
            Image::Raster(image) => image.metadata,
        }
    }

    pub fn cors_status(&self) -> CorsStatus {
        match self {
            Image::Vector(image) => image.cors_status,
            Image::Raster(image) => image.cors_status,
        }
    }

    pub fn as_raster_image(&self) -> Option<Arc<RasterImage>> {
        match self {
            Image::Raster(image) => Some(image.clone()),
            Image::Vector(..) => None,
        }
    }
}

/// Indicating either entire image or just metadata availability
#[derive(Clone, Debug, MallocSizeOf)]
pub enum ImageOrMetadataAvailable {
    ImageAvailable { image: Image, url: BrowserUrl },
    MetadataAvailable(ImageMetadata, PendingImageId),
}

pub type ImageCacheResponseCallback = Box<dyn Fn(ImageCacheResponseMessage) + Send + 'static>;

/// This is optionally passed to the image cache when requesting
/// and image, and returned to the specified event loop when the
/// image load completes. It is typically used to trigger a reflow
/// and/or repaint.
#[derive(MallocSizeOf)]
pub struct ImageLoadListener {
    pipeline_id: PipelineId,
    pub id: PendingImageId,
    #[ignore_malloc_size_of = "Difficult to measure FnOnce"]
    callback: ImageCacheResponseCallback,
}

impl ImageLoadListener {
    pub fn new(
        callback: ImageCacheResponseCallback,
        pipeline_id: PipelineId,
        id: PendingImageId,
    ) -> ImageLoadListener {
        ImageLoadListener {
            pipeline_id,
            callback,
            id,
        }
    }

    pub fn respond(&self, response: ImageResponse) {
        debug!("Notifying listener");
        (self.callback)(ImageCacheResponseMessage::NotifyPendingImageLoadStatus(
            PendingImageResponse {
                pipeline_id: self.pipeline_id,
                response,
                id: self.id,
            },
        ));
    }
}

/// The returned image.
#[derive(Clone, Debug, MallocSizeOf)]
pub enum ImageResponse {
    /// The requested image was loaded.
    Loaded(Image, BrowserUrl),
    /// The request image metadata was loaded.
    MetadataLoaded(ImageMetadata),
    /// The requested image failed to load or decode.
    FailedToLoadOrDecode,
}

/// The unique id for an image that has previously been requested.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, MallocSizeOf, PartialEq, Serialize)]
pub struct PendingImageId(pub u64);

#[derive(Clone, Debug)]
pub struct PendingImageResponse {
    pub pipeline_id: PipelineId,
    pub response: ImageResponse,
    pub id: PendingImageId,
}

#[derive(Clone, Debug)]
pub enum ImageCacheResponseMessage {
    NotifyPendingImageLoadStatus(PendingImageResponse),
}

// ======================================================================
// ImageCache public API.
// ======================================================================

pub enum ImageCacheResult {
    Available(ImageOrMetadataAvailable),
    FailedToLoadOrDecode,
    Pending(PendingImageId),
    ReadyForRequest(PendingImageId),
}

/// An [`ImageCache`] manages fetching and decoding images for a single `Pipeline` for its
/// `Document` and all of its associated `Worker`s.
pub trait ImageCache: Sync + Send {
    fn memory_reports(&self, prefix: &str, ops: &mut MallocSizeOfOps) -> Vec<Report>;

    /// Get an [`ImageKey`] to be used for external WebRender image management for
    /// things like canvas rendering. Returns `None` when an [`ImageKey`] cannot
    /// be generated properly.
    fn get_image_key(&self) -> Option<ImageKey>;

    /// Definitively check whether there is a cached, fully loaded image available.
    fn get_image(
        &self,
        url: BrowserUrl,
        origin: ImmutableOrigin,
        cors_setting: Option<CorsSettings>,
    ) -> Option<Image>;

    fn get_cached_image_status(
        &self,
        url: BrowserUrl,
        origin: ImmutableOrigin,
        cors_setting: Option<CorsSettings>,
    ) -> ImageCacheResult;

    /// Returns the original SVG bytes for a cached vector image.
    fn get_vector_image_bytes(&self, image_id: VectorImageId) -> Option<Arc<Vec<u8>>>;

    /// Removes the completed image from the image_cache, identified by url, origin, and cors
    fn evict_completed_image(
        &self,
        url: &BrowserUrl,
        origin: &ImmutableOrigin,
        cors_setting: &Option<CorsSettings>,
    );

    /// Synchronously get the broken image icon for this [`ImageCache`]. This will
    /// allocate space for this icon and upload it to WebRender.
    fn get_broken_image_icon(&self) -> Option<Arc<RasterImage>>;

    /// Add a new listener for the given pending image id. If the image is already present,
    /// the responder will still receive the expected response.
    fn add_listener(&self, listener: ImageLoadListener);

    /// Inform the image cache about a response for a pending request.
    fn notify_pending_response(&self, id: PendingImageId, action: FetchResponseMsg);

    /// Fills the image cache with a batch of keys.
    fn fill_key_cache_with_batch_of_keys(&self, image_keys: Vec<ImageKey>);
}
