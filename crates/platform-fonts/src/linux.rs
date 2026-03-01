use std::ffi::{CStr, CString};
use std::path::PathBuf;
use std::ptr;

use fontconfig_sys::constants::{
    FC_FAMILY, FC_FILE, FC_FONTFORMAT, FC_INDEX, FC_SLANT, FC_SLANT_ITALIC, FC_SLANT_OBLIQUE,
    FC_WEIGHT, FC_WEIGHT_BOLD, FC_WEIGHT_EXTRABLACK, FC_WEIGHT_REGULAR, FC_WIDTH,
    FC_WIDTH_CONDENSED, FC_WIDTH_EXPANDED, FC_WIDTH_EXTRACONDENSED, FC_WIDTH_EXTRAEXPANDED,
    FC_WIDTH_NORMAL, FC_WIDTH_SEMICONDENSED, FC_WIDTH_SEMIEXPANDED, FC_WIDTH_ULTRACONDENSED,
    FC_WIDTH_ULTRAEXPANDED,
};
use fontconfig_sys::{
    FcChar8, FcConfigGetCurrent, FcConfigGetFonts, FcConfigSubstitute, FcDefaultSubstitute,
    FcFontMatch, FcFontSetDestroy, FcFontSetList, FcMatchPattern, FcNameParse, FcObjectSetAdd,
    FcObjectSetCreate, FcObjectSetDestroy, FcPattern, FcPatternAddString, FcPatternCreate,
    FcPatternDestroy, FcPatternGetInteger, FcPatternGetString, FcResultMatch, FcSetSystem,
};
use libc::{c_char, c_int};

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

fn weight_from_pattern(pattern: *mut FcPattern) -> Option<f32> {
    let mut weight: c_int = 0;
    unsafe {
        if FcResultMatch != FcPatternGetInteger(pattern, FC_WEIGHT.as_ptr(), 0, &mut weight) {
            return None;
        }
    }
    let mapping = [
        (0., 0.),
        (FC_WEIGHT_REGULAR as f64, 400.),
        (FC_WEIGHT_BOLD as f64, 700.),
        (FC_WEIGHT_EXTRABLACK as f64, 1000.),
    ];
    Some(map_platform_values_to_style_values(&mapping, weight as f64) as f32)
}

fn stretch_from_pattern(pattern: *mut FcPattern) -> Option<f32> {
    let mut width: c_int = 0;
    unsafe {
        if FcResultMatch != FcPatternGetInteger(pattern, FC_WIDTH.as_ptr(), 0, &mut width) {
            return None;
        }
    }
    let mapping = [
        (FC_WIDTH_ULTRACONDENSED as f64, 0.5),
        (FC_WIDTH_EXTRACONDENSED as f64, 0.625),
        (FC_WIDTH_CONDENSED as f64, 0.75),
        (FC_WIDTH_SEMICONDENSED as f64, 0.875),
        (FC_WIDTH_NORMAL as f64, 1.0),
        (FC_WIDTH_SEMIEXPANDED as f64, 1.125),
        (FC_WIDTH_EXPANDED as f64, 1.25),
        (FC_WIDTH_EXTRAEXPANDED as f64, 1.50),
        (FC_WIDTH_ULTRAEXPANDED as f64, 2.00),
    ];
    Some(map_platform_values_to_style_values(&mapping, width as f64) as f32)
}

fn style_from_pattern(pattern: *mut FcPattern) -> Option<FontStyle> {
    let mut slant: c_int = 0;
    unsafe {
        if FcResultMatch != FcPatternGetInteger(pattern, FC_SLANT.as_ptr(), 0, &mut slant) {
            return None;
        }
    }
    Some(match slant {
        FC_SLANT_ITALIC => FontStyle::Italic,
        FC_SLANT_OBLIQUE => FontStyle::Oblique(14.0),
        _ => FontStyle::Normal,
    })
}

pub fn system_font_families() -> Vec<String> {
    let mut families = Vec::new();
    unsafe {
        let config = FcConfigGetCurrent();
        let font_set = FcConfigGetFonts(config, FcSetSystem);
        for i in 0..((*font_set).nfont as isize) {
            let font = (*font_set).fonts.offset(i);
            let mut format: *mut FcChar8 = ptr::null_mut();
            if FcPatternGetString(
                *font,
                FC_FONTFORMAT.as_ptr() as *mut c_char,
                0,
                &mut format,
            ) != FcResultMatch
            {
                continue;
            }
            let fontformat = CStr::from_ptr(format as *const c_char);
            if !matches!(fontformat.to_bytes(), b"TrueType" | b"CFF" | b"Type 1") {
                continue;
            }
            let mut v: c_int = 0;
            let mut family: *mut FcChar8 = ptr::null_mut();
            while FcPatternGetString(
                *font,
                FC_FAMILY.as_ptr() as *mut c_char,
                v,
                &mut family,
            ) == FcResultMatch
            {
                if let Ok(name) = CStr::from_ptr(family as *const c_char).to_str() {
                    families.push(name.to_owned());
                }
                v += 1;
            }
        }
    }
    families.sort();
    families.dedup();
    families
}

pub fn system_font_faces(family: &str) -> Vec<SystemFontFace> {
    let mut faces = Vec::new();
    unsafe {
        let config = FcConfigGetCurrent();
        let mut font_set = FcConfigGetFonts(config, FcSetSystem);
        let font_set_array_ptr = &mut font_set;
        let pattern = FcPatternCreate();
        if pattern.is_null() {
            return faces;
        }
        let Ok(family_cstr) = CString::new(family) else {
            FcPatternDestroy(pattern);
            return faces;
        };
        if FcPatternAddString(
            pattern,
            FC_FAMILY.as_ptr() as *mut c_char,
            family_cstr.as_ptr() as *const FcChar8,
        ) == 0
        {
            FcPatternDestroy(pattern);
            return faces;
        }

        let object_set = FcObjectSetCreate();
        if object_set.is_null() {
            FcPatternDestroy(pattern);
            return faces;
        }
        FcObjectSetAdd(object_set, FC_FILE.as_ptr() as *mut c_char);
        FcObjectSetAdd(object_set, FC_INDEX.as_ptr() as *mut c_char);
        FcObjectSetAdd(object_set, FC_WEIGHT.as_ptr() as *mut c_char);
        FcObjectSetAdd(object_set, FC_SLANT.as_ptr() as *mut c_char);
        FcObjectSetAdd(object_set, FC_WIDTH.as_ptr() as *mut c_char);

        let matches = FcFontSetList(config, font_set_array_ptr, 1, pattern, object_set);

        for i in 0..((*matches).nfont as isize) {
            let font = (*matches).fonts.offset(i);

            let mut path: *mut FcChar8 = ptr::null_mut();
            if FcPatternGetString(*font, FC_FILE.as_ptr() as *mut c_char, 0, &mut path)
                != FcResultMatch
            {
                continue;
            }

            let mut index: c_int = 0;
            if FcPatternGetInteger(*font, FC_INDEX.as_ptr() as *mut c_char, 0, &mut index)
                != FcResultMatch
            {
                continue;
            }

            let Some(weight) = weight_from_pattern(*font) else {
                continue;
            };
            let Some(stretch) = stretch_from_pattern(*font) else {
                continue;
            };
            let Some(style) = style_from_pattern(*font) else {
                continue;
            };

            let Ok(path_str) = CStr::from_ptr(path as *const c_char).to_str() else {
                continue;
            };

            faces.push(SystemFontFace {
                path: PathBuf::from(path_str),
                index: index as u32,
                weight,
                stretch,
                style,
            });
        }

        FcFontSetDestroy(matches);
        FcPatternDestroy(pattern);
        FcObjectSetDestroy(object_set);
    }
    faces
}

pub fn default_generic_family(generic: GenericFamily) -> String {
    let generic_string = match generic {
        GenericFamily::Serif => c"serif",
        GenericFamily::SansSerif | GenericFamily::SystemUi => c"sans-serif",
        GenericFamily::Monospace => c"monospace",
        GenericFamily::Cursive => c"cursive",
        GenericFamily::Fantasy => c"fantasy",
    };

    unsafe {
        let pattern = FcNameParse(generic_string.as_ptr() as *mut FcChar8);
        FcConfigSubstitute(ptr::null_mut(), pattern, FcMatchPattern);
        FcDefaultSubstitute(pattern);

        let mut result = 0;
        let family_match = FcFontMatch(ptr::null_mut(), pattern, &mut result);

        if result == FcResultMatch {
            let mut match_string: *mut FcChar8 = ptr::null_mut();
            FcPatternGetString(
                family_match,
                FC_FAMILY.as_ptr() as *mut c_char,
                0,
                &mut match_string,
            );
            let name = CStr::from_ptr(match_string as *const c_char)
                .to_str()
                .expect("Font family name contains invalid UTF-8")
                .to_owned();
            FcPatternDestroy(family_match);
            FcPatternDestroy(pattern);
            return name;
        }

        FcPatternDestroy(family_match);
        FcPatternDestroy(pattern);
    }

    match generic {
        GenericFamily::Serif => "Noto Serif",
        GenericFamily::SansSerif | GenericFamily::SystemUi => "Noto Sans",
        GenericFamily::Monospace => "Deja Vu Sans Mono",
        GenericFamily::Cursive => "Comic Sans MS",
        GenericFamily::Fantasy => "Impact",
    }
    .to_owned()
}
