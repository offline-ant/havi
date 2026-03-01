use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use havi_platform_fonts::GenericFamily;

pub use matching::FontRequest;
pub use havi_platform_fonts::FontStyle;

mod catalog;
mod matching;

/// Opaque handle identifying a resolved font.
#[derive(Clone, Debug, Hash, PartialEq, Eq)]
pub struct FontHandle {
    pub path: PathBuf,
    pub index: u32,
}

/// Font metrics in em units (divide by units_per_em to get ems).
#[derive(Clone, Debug)]
pub struct ResolvedFontMetrics {
    pub units_per_em: f32,
    pub ascender: f32,
    pub descender: f32,
    pub line_gap: f32,
    pub underline_position: f32,
    pub underline_thickness: f32,
    pub strikeout_position: f32,
    pub strikeout_thickness: f32,
}

/// Shared font data bytes.
pub type FontData = Arc<Vec<u8>>;

/// Key for a resolved font family.
#[derive(Clone, Debug, Hash, PartialEq, Eq)]
struct FontFamilyKey {
    families: Vec<String>,
    weight: u32,
    stretch: u32,
    italic: bool,
}

pub struct HaviFonts {
    catalog: catalog::FontCatalog,
    /// path+index → loaded font data
    loaded_data: HashMap<(PathBuf, u32), FontData>,
    /// Resolved family cache: key → list of font handles
    resolved_families: HashMap<FontFamilyKey, Vec<FontHandle>>,
}

impl HaviFonts {
    pub fn new() -> Self {
        Self {
            catalog: catalog::FontCatalog::new(),
            loaded_data: HashMap::new(),
            resolved_families: HashMap::new(),
        }
    }

    /// Resolve CSS font-family list + weight/stretch/style to a list of FontHandles.
    /// The first handle is the best match; others are fallbacks.
    pub fn resolve_family(
        &mut self,
        families: &[&str],
        request: &FontRequest,
    ) -> Vec<FontHandle> {
        let key = FontFamilyKey {
            families: families.iter().map(|s| s.to_lowercase()).collect(),
            weight: (request.weight * 10.0) as u32,
            stretch: (request.stretch * 1000.0) as u32,
            italic: matches!(request.style, havi_platform_fonts::FontStyle::Italic),
        };

        if let Some(handles) = self.resolved_families.get(&key) {
            return handles.clone();
        }

        let mut handles = Vec::new();

        for family_name in families {
            let resolved_name = if let Some(generic) = is_generic_family(family_name) {
                let name = self.catalog.resolve_generic(generic);
                if name.is_empty() {
                    continue;
                }
                name.to_string()
            } else {
                family_name.to_lowercase()
            };

            let faces = match self.catalog.faces(&resolved_name) {
                Some(faces) => faces,
                None => continue,
            };

            let best_idx = match matching::match_font(faces, request) {
                Some(idx) => idx,
                None => continue,
            };

            let face = &faces[best_idx];
            let handle = FontHandle {
                path: face.path.clone(),
                index: face.index,
            };
            if !handles.contains(&handle) {
                handles.push(handle);
            }
        }

        self.resolved_families.insert(key, handles.clone());
        handles
    }

    /// Load font data bytes for a handle. Caches the result.
    pub fn load_data(&mut self, handle: &FontHandle) -> Option<FontData> {
        let key = (handle.path.clone(), handle.index);
        if let Some(data) = self.loaded_data.get(&key) {
            return Some(data.clone());
        }
        let bytes = std::fs::read(&handle.path).ok()?;
        let data: FontData = Arc::new(bytes);
        self.loaded_data.insert(key, data.clone());
        Some(data)
    }

    /// Get font metrics for a handle.
    pub fn metrics(&mut self, handle: &FontHandle) -> Option<ResolvedFontMetrics> {
        let data = self.load_data(handle)?;
        let face = ttf_parser::Face::parse(&data, handle.index).ok()?;
        let upem = face.units_per_em() as f32;
        let ul = face.underline_metrics();
        let so = face.strikeout_metrics();
        Some(ResolvedFontMetrics {
            units_per_em: upem,
            ascender: face.ascender() as f32,
            descender: face.descender() as f32,
            line_gap: face.line_gap() as f32,
            underline_position: ul.map_or(0.0, |m| m.position as f32),
            underline_thickness: ul.map_or(0.0, |m| m.thickness as f32),
            strikeout_position: so.map_or(0.0, |m| m.position as f32),
            strikeout_thickness: so.map_or(0.0, |m| m.thickness as f32),
        })
    }

    /// Create a rustybuzz face for shaping. Caller must hold the FontData alive.
    pub fn rustybuzz_face<'a>(&mut self, handle: &FontHandle, data: &'a [u8]) -> Option<rustybuzz::Face<'a>> {
        rustybuzz::Face::from_slice(data, handle.index)
    }
}

fn is_generic_family(name: &str) -> Option<GenericFamily> {
    match name.to_lowercase().as_str() {
        "serif" => Some(GenericFamily::Serif),
        "sans-serif" => Some(GenericFamily::SansSerif),
        "monospace" => Some(GenericFamily::Monospace),
        "cursive" => Some(GenericFamily::Cursive),
        "fantasy" => Some(GenericFamily::Fantasy),
        "system-ui" => Some(GenericFamily::SystemUi),
        _ => None,
    }
}
