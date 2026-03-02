use std::ffi::c_void;
use std::ptr;

use objc2_core_foundation::{CFDictionary, CFNumber, CFRetained, CFSet, CFString, CFType, CFURL};
use objc2_core_text::{
    CTFontDescriptor, CTFontManagerCopyAvailableFontFamilyNames, CTFontSymbolicTraits,
    kCTFontFamilyNameAttribute, kCTFontSlantTrait, kCTFontSymbolicTrait, kCTFontTraitsAttribute,
    kCTFontURLAttribute, kCTFontWeightTrait, kCTFontWidthTrait,
};

use crate::unicode_block::{UnicodeBlock, UnicodeBlockMethod};
use crate::{FontStyle, GenericFamily, SystemFontFace};

/// Linear interpolation between platform and CSS values.
fn map_platform_values_to_style_values(mapping: &[(f64, f64)], value: f64) -> f64 {
    if value < mapping[0].0 {
        return mapping[0].1;
    }
    for window in mapping.windows(2) {
        let (pa, ca) = window[0];
        let (pb, cb) = window[1];
        if value >= pa && value <= pb {
            let ratio = (value - pa) / (pb - pa);
            return ca + (cb - ca) * ratio;
        }
    }
    mapping[mapping.len() - 1].1
}

fn face_from_descriptor(descriptor: &CTFontDescriptor) -> Option<SystemFontFace> {
    let url = unsafe {
        descriptor
            .attribute(kCTFontURLAttribute)?
            .downcast::<CFURL>()
            .ok()?
    };
    let path = url.to_file_path()?;

    let traits = unsafe {
        descriptor
            .attribute(kCTFontTraitsAttribute)?
            .downcast::<CFDictionary>()
            .ok()?
    };
    let traits = unsafe { traits.cast_unchecked::<CFString, CFType>() };

    let get_f64_trait = |key| {
        traits
            .get(key)
            .and_then(|value| value.downcast::<CFNumber>().ok()?.as_f64())
    };

    // Weight: CoreText -1.0..1.0 -> CSS 0..1000
    let font_weight = get_f64_trait(unsafe { kCTFontWeightTrait }).unwrap_or(0.);
    let weight =
        map_platform_values_to_style_values(&[(-1., 0.), (0., 400.), (1., 1000.)], font_weight)
            as f32;

    // Stretch: CoreText -1.0..1.0, +1.0 gives percentage where 1.0 = normal
    let font_stretch = get_f64_trait(unsafe { kCTFontWidthTrait }).unwrap_or(0.);
    let stretch = font_stretch as f32 + 1.0;

    // Style: check slant trait, fall back to symbolic italic flag
    let font_slant = get_f64_trait(unsafe { kCTFontSlantTrait }).unwrap_or(0.);
    let style = if font_slant == 0. {
        let symbolic_traits = traits
            .get(unsafe { kCTFontSymbolicTrait })
            .and_then(|value| value.downcast::<CFNumber>().ok()?.as_i64())
            .map(|value| CTFontSymbolicTraits::from_bits_retain(value as u32));
        match symbolic_traits {
            Some(t) if t.contains(CTFontSymbolicTraits::TraitItalic) => FontStyle::Italic,
            _ => FontStyle::Normal,
        }
    } else {
        let degrees = map_platform_values_to_style_values(
            &[(-1., -30.), (0., 0.), (1., 30.)],
            font_slant,
        ) as f32;
        FontStyle::Oblique(degrees)
    };

    Some(SystemFontFace {
        path,
        index: 0,
        weight,
        stretch,
        style,
    })
}

pub fn system_font_families() -> Vec<String> {
    let family_names = unsafe { CTFontManagerCopyAvailableFontFamilyNames() };
    let family_names = unsafe { family_names.cast_unchecked::<CFString>() };
    let mut families: Vec<String> = family_names.iter().map(|name| name.to_string()).collect();
    families.sort();
    families.dedup();
    families
}

pub fn system_font_faces(family: &str) -> Vec<SystemFontFace> {
    let specified_attributes: CFRetained<CFDictionary<CFString, CFType>> =
        CFDictionary::from_slices(
            &[unsafe { kCTFontFamilyNameAttribute }],
            &[CFString::from_str(family).as_ref()],
        );
    let wildcard_descriptor =
        unsafe { CTFontDescriptor::with_attributes(specified_attributes.as_ref()) };

    let values = [unsafe { kCTFontFamilyNameAttribute }];
    let values = values.as_ptr().cast::<*const c_void>().cast_mut();
    let mandatory_attributes = unsafe { CFSet::new(None, values, 1, ptr::null()) };
    let Some(mandatory_attributes) = mandatory_attributes else {
        return Vec::new();
    };

    let matched_descriptors =
        unsafe { wildcard_descriptor.matching_font_descriptors(Some(&mandatory_attributes)) };
    let Some(matched_descriptors) = matched_descriptors else {
        return Vec::new();
    };
    let matched_descriptors = unsafe { matched_descriptors.cast_unchecked::<CTFontDescriptor>() };

    matched_descriptors
        .iter()
        .filter_map(|desc| face_from_descriptor(&desc))
        .collect()
}

pub fn default_generic_family(generic: GenericFamily) -> String {
    match generic {
        GenericFamily::Serif => "Times",
        GenericFamily::SansSerif | GenericFamily::SystemUi => "Helvetica",
        GenericFamily::Monospace => "Menlo",
        GenericFamily::Cursive => "Apple Chancery",
        GenericFamily::Fantasy => "Papyrus",
    }
    .to_owned()
}

pub fn fallback_families(ch: char) -> Vec<&'static str> {
    let mut families = Vec::new();

    if crate::is_emoji_presentation(ch) {
        families.push("Apple Color Emoji");
    }

    let script = unicode_script::Script::from(ch);
    if let Some(block) = ch.block() {
        match block {
            _ if matches!(
                script,
                unicode_script::Script::Common
                    | unicode_script::Script::Inherited
                    | unicode_script::Script::Latin
                    | unicode_script::Script::Cyrillic
                    | unicode_script::Script::Greek
            ) =>
            {
                families.push("Lucida Grande");
            }
            _ if matches!(script, unicode_script::Script::Bopomofo | unicode_script::Script::Han) =>
            {
                families.push("Songti SC");
                if ch as u32 > 0x10000 {
                    families.push("SimSun-ExtB");
                }
            }
            UnicodeBlock::Hiragana
            | UnicodeBlock::Katakana
            | UnicodeBlock::KatakanaPhoneticExtensions => {
                families.push("Hiragino Sans");
                families.push("Hiragino Kaku Gothic ProN");
            }
            UnicodeBlock::HangulJamo
            | UnicodeBlock::HangulJamoExtendedA
            | UnicodeBlock::HangulJamoExtendedB
            | UnicodeBlock::HangulCompatibilityJamo
            | UnicodeBlock::HangulSyllables => {
                families.push("Nanum Gothic");
                families.push("Apple SD Gothic Neo");
            }
            UnicodeBlock::Arabic => families.push("Geeza Pro"),
            UnicodeBlock::Armenian => families.push("Mshtakan"),
            UnicodeBlock::Bengali => families.push("Bangla Sangam MN"),
            UnicodeBlock::Cherokee => families.push("Plantagenet Cherokee"),
            UnicodeBlock::Deseret => families.push("Baskerville"),
            UnicodeBlock::Devanagari | UnicodeBlock::DevanagariExtended => {
                families.push("Devanagari Sangam MN")
            }
            UnicodeBlock::Ethiopic
            | UnicodeBlock::EthiopicExtended
            | UnicodeBlock::EthiopicExtendedA
            | UnicodeBlock::EthiopicSupplement => families.push("Kefa"),
            UnicodeBlock::Georgian | UnicodeBlock::GeorgianSupplement => {
                families.push("Helvetica")
            }
            UnicodeBlock::Gujarati => families.push("Gujarati Sangam MN"),
            UnicodeBlock::Gurmukhi => families.push("Gurmukhi MN"),
            UnicodeBlock::Hebrew => families.push("Lucida Grande"),
            UnicodeBlock::Kannada => families.push("Kannada MN"),
            UnicodeBlock::Khmer => families.push("Khmer MN"),
            UnicodeBlock::Lao => families.push("Lao MN"),
            UnicodeBlock::Malayalam => families.push("Malayalam Sangam MN"),
            UnicodeBlock::Myanmar
            | UnicodeBlock::MyanmarExtendedA
            | UnicodeBlock::MyanmarExtendedB => families.push("Myanmar MN"),
            UnicodeBlock::Oriya => families.push("Oriya Sangam MN"),
            UnicodeBlock::Sinhala | UnicodeBlock::SinhalaArchaicNumbers => {
                families.push("Sinhala Sangam MN")
            }
            UnicodeBlock::Tamil => families.push("Tamil MN"),
            UnicodeBlock::Telugu => families.push("Telugu MN"),
            UnicodeBlock::Thaana => families.push("Thonburi"),
            UnicodeBlock::Tibetan => families.push("Kailasa"),
            UnicodeBlock::UnifiedCanadianAboriginalSyllabics
            | UnicodeBlock::UnifiedCanadianAboriginalSyllabicsExtended => {
                families.push("Euphemia UCAS")
            }
            UnicodeBlock::YiSyllables | UnicodeBlock::YiRadicals => {
                families.push("STHeiti");
            }
            UnicodeBlock::BraillePatterns => families.push("Apple Braille"),
            _ => {}
        }
    }

    crate::add_noto_fallback_families(ch, &mut families);

    // Supplementary Multilingual Plane fallbacks
    let plane = (ch as u32) >> 16;
    if plane == 1 {
        let block_prefix = (ch as u32) >> 8;
        if block_prefix == 0x27 {
            families.push("Zapf Dingbats");
        }
        families.push("Geneva");
        families.push("Apple Symbols");
        families.push("STIXGeneral");
        families.push("Hiragino Sans");
        families.push("Hiragino Kaku Gothic ProN");
    }

    families.push("Arial Unicode MS");
    families
}
