/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

use std::sync::Arc;

use base::generic_channel::GenericSharedMemory;
use base::id::PainterId;
use webrender_api::{FontInstanceFlags, FontInstanceKey, FontKey, FontVariation, NativeFontHandle};

pub trait FontRenderBackend: Send + Sync {
    fn add_font(&self, font_key: FontKey, data: Arc<GenericSharedMemory>, index: u32);
    fn add_system_font(&self, font_key: FontKey, handle: NativeFontHandle);
    fn add_font_instance(
        &self,
        font_instance_key: FontInstanceKey,
        font_key: FontKey,
        size: f32,
        flags: FontInstanceFlags,
        variations: Vec<FontVariation>,
    );
    fn fetch_font_keys(
        &self,
        number_of_font_keys: usize,
        number_of_font_instance_keys: usize,
        painter_id: PainterId,
    ) -> (Vec<FontKey>, Vec<FontInstanceKey>);
}

#[derive(Clone)]
pub struct FontRenderApi(Arc<dyn FontRenderBackend>);

impl FontRenderApi {
    pub fn new(backend: Arc<dyn FontRenderBackend>) -> Self {
        Self(backend)
    }

    pub fn dummy() -> Self {
        Self::new(Arc::new(DummyFontRenderBackend))
    }

    pub fn add_font(&self, font_key: FontKey, data: Arc<GenericSharedMemory>, index: u32) {
        self.0.add_font(font_key, data, index)
    }

    pub fn add_system_font(&self, font_key: FontKey, handle: NativeFontHandle) {
        self.0.add_system_font(font_key, handle)
    }

    pub fn add_font_instance(
        &self,
        font_instance_key: FontInstanceKey,
        font_key: FontKey,
        size: f32,
        flags: FontInstanceFlags,
        variations: Vec<FontVariation>,
    ) {
        self.0
            .add_font_instance(font_instance_key, font_key, size, flags, variations)
    }

    pub fn fetch_font_keys(
        &self,
        number_of_font_keys: usize,
        number_of_font_instance_keys: usize,
        painter_id: PainterId,
    ) -> (Vec<FontKey>, Vec<FontInstanceKey>) {
        self.0.fetch_font_keys(
            number_of_font_keys,
            number_of_font_instance_keys,
            painter_id,
        )
    }
}

struct DummyFontRenderBackend;

impl FontRenderBackend for DummyFontRenderBackend {
    fn add_font(&self, _font_key: FontKey, _data: Arc<GenericSharedMemory>, _index: u32) {}

    fn add_system_font(&self, _font_key: FontKey, _handle: NativeFontHandle) {}

    fn add_font_instance(
        &self,
        _font_instance_key: FontInstanceKey,
        _font_key: FontKey,
        _size: f32,
        _flags: FontInstanceFlags,
        _variations: Vec<FontVariation>,
    ) {
    }

    fn fetch_font_keys(
        &self,
        _number_of_font_keys: usize,
        _number_of_font_instance_keys: usize,
        _painter_id: PainterId,
    ) -> (Vec<FontKey>, Vec<FontInstanceKey>) {
        (Vec::new(), Vec::new())
    }
}
