/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

#![deny(unsafe_code)]

use std::sync::Arc;

pub use ::fonts::*;

use base::generic_channel::GenericSharedMemory;
use base::id::PainterId;
use webrender_api::{
    FontInstanceFlags, FontInstanceKey, FontKey, FontVariation, NativeFontHandle,
};

use crate::paint::CrossProcessPaintApi;

pub(crate) fn font_render_api_from_paint_api(paint_api: CrossProcessPaintApi) -> FontRenderApi {
    FontRenderApi::new(Arc::new(PaintFontRenderBackend { paint_api }))
}

struct PaintFontRenderBackend {
    paint_api: CrossProcessPaintApi,
}

impl FontRenderBackend for PaintFontRenderBackend {
    fn add_font(&self, font_key: FontKey, data: Arc<GenericSharedMemory>, index: u32) {
        self.paint_api.add_font(font_key, data, index);
    }

    fn add_system_font(&self, font_key: FontKey, handle: NativeFontHandle) {
        self.paint_api.add_system_font(font_key, handle);
    }

    fn add_font_instance(
        &self,
        font_instance_key: FontInstanceKey,
        font_key: FontKey,
        size: f32,
        flags: FontInstanceFlags,
        variations: Vec<FontVariation>,
    ) {
        self.paint_api
            .add_font_instance(font_instance_key, font_key, size, flags, variations);
    }

    fn fetch_font_keys(
        &self,
        number_of_font_keys: usize,
        number_of_font_instance_keys: usize,
        painter_id: PainterId,
    ) -> (Vec<FontKey>, Vec<FontInstanceKey>) {
        self.paint_api.fetch_font_keys(
            number_of_font_keys,
            number_of_font_instance_keys,
            painter_id,
        )
    }
}
