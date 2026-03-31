/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

#![deny(unsafe_code)]

mod font;
mod font_context;
mod font_descriptor;
mod font_identifier;
mod font_render_api;
mod font_store;
mod font_template;
mod glyph;
mod system_font_service_proxy;
#[expect(unsafe_code)]
pub mod platform; // Public because integration tests need this
mod shapers;
mod system_font_service;

use std::ops::{Deref, Range};
use std::sync::Arc;

use base::generic_channel::GenericSharedMemory;
use malloc_size_of_derive::MallocSizeOf;
use num_derive::{NumOps, One, Zero};
use serde::{Deserialize, Serialize};
use webrender_api::euclid::num::One;

pub(crate) use font::*;
// These items are not meant to be part of the public API but are used for integration tests
pub use font::{Font, FontFamilyDescriptor, FontSearchScope, PlatformFontMethods};
pub use font::{
    FontBaseline, FontGroup, FontMetrics, FontRef, LAST_RESORT_GLYPH_ADVANCE, ShapingFlags,
    ShapingOptions,
};
pub use font_context::{
    CspViolationHandler, FontContext, FontContextWebFontMethods, NetworkTimingHandler,
    WebFontDocumentContext,
};
pub use font_descriptor::*;
pub use font_identifier::*;
pub use font_render_api::{FontRenderApi, FontRenderBackend};
pub use font_store::FontTemplates;
pub use font_template::*;
pub use system_font_service_proxy::*;
pub(crate) use glyph::*;
pub use glyph::{GlyphInfo, GlyphStore};
pub use platform::font_list::fallback_font_families;
pub(crate) use shapers::*;
use style::values::computed::XLang;
pub use system_font_service::SystemFontService;
use unicode_properties::{EmojiStatus, UnicodeEmoji, emoji};

#[derive(
    Clone,
    Copy,
    Debug,
    Default,
    Deserialize,
    Eq,
    MallocSizeOf,
    NumOps,
    Ord,
    One,
    PartialEq,
    PartialOrd,
    Serialize,
    Zero,
)]
pub struct ByteIndex(pub usize);

impl ByteIndex {
    pub fn get(&self) -> usize {
        self.0
    }
}

#[derive(Clone, Debug, Default, Deserialize, MallocSizeOf, PartialEq, Serialize)]
pub struct TextByteRange(Range<ByteIndex>);

impl TextByteRange {
    pub fn len(&self) -> ByteIndex {
        self.0.end - self.0.start
    }

    #[inline]
    pub fn intersect(&self, other: &Self) -> Self {
        let begin = self.start.max(other.start);
        let end = self.end.min(other.end);

        if end < begin {
            Self::default()
        } else {
            Self::new(begin, end)
        }
    }

    #[inline]
    pub fn contains_inclusive(&self, index: ByteIndex) -> bool {
        index >= self.start && index <= self.end
    }
}

impl Deref for TextByteRange {
    type Target = Range<ByteIndex>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl Iterator for TextByteRange {
    type Item = ByteIndex;

    fn next(&mut self) -> Option<Self::Item> {
        if self.0.start == self.0.end {
            None
        } else {
            let next = self.0.start;
            self.0.start = self.0.start + ByteIndex::one();
            Some(next)
        }
    }
}

impl DoubleEndedIterator for TextByteRange {
    fn next_back(&mut self) -> Option<Self::Item> {
        if self.0.start == self.0.end {
            None
        } else {
            self.0.end = self.0.end - ByteIndex::one();
            Some(self.0.end)
        }
    }
}

impl TextByteRange {
    pub fn new(start: ByteIndex, end: ByteIndex) -> Self {
        Self(start..end)
    }

    pub fn iter(&self) -> Range<ByteIndex> {
        self.0.clone()
    }
}

pub type StylesheetWebFontLoadFinishedCallback = Arc<dyn Fn(bool) + Send + Sync + 'static>;

#[derive(Clone, Deserialize, MallocSizeOf, Serialize)]
pub struct FontData(#[conditional_malloc_size_of] pub(crate) Arc<GenericSharedMemory>);

impl FontData {
    pub fn from_bytes(bytes: &[u8]) -> Self {
        Self(Arc::new(GenericSharedMemory::from_bytes(bytes)))
    }

    pub fn as_ipc_shared_memory(&self) -> Arc<GenericSharedMemory> {
        self.0.clone()
    }
}

impl AsRef<[u8]> for FontData {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

#[derive(Deserialize, Clone, Serialize)]
pub struct FontDataAndIndex {
    pub data: FontData,
    pub index: u32,
}

#[derive(Copy, Clone, PartialEq)]
pub enum FontDataError {
    FailedToLoad,
}

/// Whether or not font fallback selection prefers the emoji or text representation
/// of a character. If `None` then either presentation is acceptable.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum EmojiPresentationPreference {
    None,
    Text,
    Emoji,
}

#[derive(Clone, Debug)]
pub struct FallbackFontSelectionOptions {
    pub(crate) character: char,
    pub(crate) presentation_preference: EmojiPresentationPreference,
    pub(crate) lang: XLang,
}

impl Default for FallbackFontSelectionOptions {
    fn default() -> Self {
        Self {
            character: ' ',
            presentation_preference: EmojiPresentationPreference::None,
            lang: XLang::get_initial_value(),
        }
    }
}

impl FallbackFontSelectionOptions {
    pub(crate) fn new(character: char, next_character: Option<char>, lang: XLang) -> Self {
        let presentation_preference = match next_character {
            Some(next_character) if emoji::is_emoji_presentation_selector(next_character) => {
                EmojiPresentationPreference::Emoji
            },
            Some(next_character) if emoji::is_text_presentation_selector(next_character) => {
                EmojiPresentationPreference::Text
            },
            // We don't want to select emoji prsentation for any possible character that might be an emoji, because
            // that includes characters such as '0' that are also used outside of emoji clusters. Instead, only
            // select the emoji font for characters that explicitly have an emoji presentation (in the absence
            // of the emoji presentation selectors above).
            _ if matches!(
                character.emoji_status(),
                EmojiStatus::EmojiPresentation |
                    EmojiStatus::EmojiPresentationAndModifierBase |
                    EmojiStatus::EmojiPresentationAndEmojiComponent |
                    EmojiStatus::EmojiPresentationAndModifierAndEmojiComponent
            ) =>
            {
                EmojiPresentationPreference::Emoji
            },
            _ if character.is_emoji_char() => EmojiPresentationPreference::Text,
            _ => EmojiPresentationPreference::None,
        };
        Self {
            character,
            presentation_preference,
            lang,
        }
    }
}

pub(crate) fn float_to_fixed(before: usize, f: f64) -> i32 {
    ((1i32 << before) as f64 * f) as i32
}

pub(crate) fn fixed_to_float(before: usize, f: i32) -> f64 {
    f as f64 * 1.0f64 / ((1i32 << before) as f64)
}
