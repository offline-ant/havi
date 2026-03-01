use std::collections::HashMap;

use havi_platform_fonts::{
    default_generic_family, system_font_faces, system_font_families, GenericFamily, SystemFontFace,
};

pub struct FontCatalog {
    /// family name (lowercase) → list of faces
    families: HashMap<String, Vec<SystemFontFace>>,
    /// Generic family defaults (lowercase family name)
    generic_defaults: HashMap<GenericFamily, String>,
}

impl FontCatalog {
    pub fn new() -> Self {
        let mut families = HashMap::new();
        for family in system_font_families() {
            let faces = system_font_faces(&family);
            if !faces.is_empty() {
                families.insert(family.to_lowercase(), faces);
            }
        }

        let mut generic_defaults = HashMap::new();
        for generic in [
            GenericFamily::Serif,
            GenericFamily::SansSerif,
            GenericFamily::Monospace,
            GenericFamily::Cursive,
            GenericFamily::Fantasy,
            GenericFamily::SystemUi,
        ] {
            let name = default_generic_family(generic);
            if !name.is_empty() {
                generic_defaults.insert(generic, name.to_lowercase());
            }
        }

        Self {
            families,
            generic_defaults,
        }
    }

    pub fn faces(&self, family: &str) -> Option<&[SystemFontFace]> {
        self.families.get(&family.to_lowercase()).map(|v| v.as_slice())
    }

    pub fn resolve_generic(&self, generic: GenericFamily) -> &str {
        self.generic_defaults
            .get(&generic)
            .map(|s| s.as_str())
            .unwrap_or("")
    }
}
