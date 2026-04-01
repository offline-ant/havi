/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

use std::collections::HashMap;
use std::sync::Arc;

use base::id::PainterId;
use crate::fonts::FontContext;
use crate::layout::{
    AnimatingImages, IFrameSizes, LayoutImageDestination, PendingImage, PendingImageState,
};
use crate::net::image_cache::{
    Image as CachedImage, ImageCache, ImageCacheResult, ImageOrMetadataAvailable, PendingImageId,
};
use parking_lot::{Mutex, RwLock};
use crate::pixels::RasterImage;
use servo_url::{ImmutableOrigin, BrowserUrl};
use style::context::SharedStyleContext;
use style::dom::OpaqueNode;

pub(crate) type CachedImageOrError = Result<CachedImage, ResolveImageError>;

pub(crate) struct LayoutContext<'a> {
    pub use_rayon: bool,

    /// Bits shared by the layout and style system.
    pub style_context: SharedStyleContext<'a>,

    /// A FontContext to be used during layout.
    pub font_context: Arc<FontContext>,

    /// A collection of `<iframe>` sizes to send back to script.
    pub iframe_sizes: Mutex<IFrameSizes>,

    /// An [`ImageResolver`] used for resolving images during box and fragment
    /// tree construction. Later passed to display list construction.
    pub image_resolver: Arc<ImageResolver>,

    /// The [`PainterId`] that identifies which embedder-owned paint target this layout targets.
    pub painter_id: PainterId,
}

#[derive(Clone, Copy, Debug)]
pub enum ResolveImageError {
    LoadError,
    ImagePending,
    OnlyMetadata,
}

pub(crate) enum LayoutImageCacheResult {
    Pending,
    DataAvailable(ImageOrMetadataAvailable),
    LoadError,
}

pub(crate) struct ImageResolver {
    /// The origin of the `Document` that this [`ImageResolver`] resolves images for.
    pub origin: ImmutableOrigin,

    /// Reference to the script thread image cache.
    pub image_cache: Arc<dyn ImageCache>,

    /// A list of in-progress image loads to be shared with the script thread.
    pub pending_images: Mutex<Vec<PendingImage>>,

    /// A shared reference to script's map of DOM nodes with animated images. This is used
    /// to manage image animations in script and inform the script about newly animating
    /// nodes.
    pub animating_images: Arc<RwLock<AnimatingImages>>,

    // A cache that maps image resources used in CSS (e.g as the `url()` value
    // for `background-image` or `content` property) to the final resolved image data.
    pub resolved_images_cache: Arc<RwLock<HashMap<BrowserUrl, CachedImageOrError>>>,

    /// The current animation timeline value used to properly initialize animating images.
    pub animation_timeline_value: f64,
}

impl Drop for ImageResolver {
    fn drop(&mut self) {
        if !std::thread::panicking() {
            assert!(self.pending_images.lock().is_empty());
        }
    }
}

impl ImageResolver {
    pub(crate) fn get_or_request_image_or_meta(
        &self,
        node: OpaqueNode,
        url: BrowserUrl,
        destination: LayoutImageDestination,
    ) -> LayoutImageCacheResult {
        // Check for available image or start tracking.
        let cache_result =
            self.image_cache
                .get_cached_image_status(url.clone(), self.origin.clone(), None);

        match cache_result {
            ImageCacheResult::Available(img_or_meta) => {
                LayoutImageCacheResult::DataAvailable(img_or_meta)
            },
            // Image has been requested, is still pending. Return no image for this paint loop.
            // When the image loads it will trigger a reflow and/or repaint.
            ImageCacheResult::Pending(id) => {
                let image = PendingImage {
                    state: PendingImageState::PendingResponse,
                    node: node.into(),
                    id,
                    origin: self.origin.clone(),
                    destination,
                };
                self.pending_images.lock().push(image);
                LayoutImageCacheResult::Pending
            },
            // Not yet requested - request image or metadata from the cache
            ImageCacheResult::ReadyForRequest(id) => {
                let image = PendingImage {
                    state: PendingImageState::Unrequested(url),
                    node: node.into(),
                    id,
                    origin: self.origin.clone(),
                    destination,
                };
                self.pending_images.lock().push(image);
                LayoutImageCacheResult::Pending
            },
            // Image failed to load, so just return the same error.
            ImageCacheResult::FailedToLoadOrDecode => LayoutImageCacheResult::LoadError,
        }
    }

    pub(crate) fn handle_animated_image(&self, node: OpaqueNode, image: Arc<RasterImage>) {
        let mut animating_images = self.animating_images.write();
        if !image.should_animate() {
            animating_images.remove(node);
        } else {
            animating_images.maybe_insert_or_update(node, image, self.animation_timeline_value);
        }
    }

    pub(crate) fn get_cached_image_for_url(
        &self,
        node: OpaqueNode,
        url: BrowserUrl,
        destination: LayoutImageDestination,
    ) -> Result<CachedImage, ResolveImageError> {
        if let Some(cached_image) = self.resolved_images_cache.read().get(&url) {
            return cached_image.clone();
        }

        let result = self.get_or_request_image_or_meta(node, url.clone(), destination);
        match result {
            LayoutImageCacheResult::DataAvailable(img_or_meta) => match img_or_meta {
                ImageOrMetadataAvailable::ImageAvailable { image, .. } => {
                    if let Some(image) = image.as_raster_image() {
                        self.handle_animated_image(node, image.clone());
                    }

                    let mut resolved_images_cache = self.resolved_images_cache.write();
                    resolved_images_cache.insert(url, Ok(image.clone()));
                    Ok(image)
                },
                ImageOrMetadataAvailable::MetadataAvailable(..) => {
                    Result::Err(ResolveImageError::OnlyMetadata)
                },
            },
            LayoutImageCacheResult::Pending => Result::Err(ResolveImageError::ImagePending),
            LayoutImageCacheResult::LoadError => {
                let error = Err(ResolveImageError::LoadError);
                self.resolved_images_cache
                    .write()
                    .insert(url, error.clone());
                error
            },
        }
    }

    pub(crate) fn vector_image_bytes(&self, image_id: PendingImageId) -> Option<Arc<Vec<u8>>> {
        self.image_cache.get_vector_image_bytes(image_id)
    }
}
