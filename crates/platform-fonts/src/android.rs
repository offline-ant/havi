use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use crate::unicode_block::{UnicodeBlock, UnicodeBlockMethod};
use crate::{FontStyle, GenericFamily, SystemFontFace};

// ---------------------------------------------------------------------------
// Inline XML parser (ported from Servo's android/xml.rs)
// ---------------------------------------------------------------------------

mod xml_parser {
    use xml::attribute::OwnedAttribute as Attribute;
    use xml::reader::XmlEvent;

    pub(super) use Attribute as XmlAttribute;

    pub(super) enum Node {
        Element {
            name: xml::name::OwnedName,
            attributes: Vec<Attribute>,
            children: Vec<Node>,
        },
        Text(String),
    }

    pub(super) fn parse(bytes: &[u8]) -> Result<Vec<Node>, xml::reader::Error> {
        let mut stack = Vec::new();
        let mut nodes = Vec::new();
        for result in xml::EventReader::new(bytes) {
            match result? {
                XmlEvent::StartElement {
                    name, attributes, ..
                } => {
                    stack.push((name, attributes, nodes));
                    nodes = Vec::new();
                }
                XmlEvent::EndElement { .. } => {
                    if let Some((name, attributes, mut parent_nodes)) = stack.pop() {
                        parent_nodes.push(Node::Element {
                            name,
                            attributes,
                            children: nodes,
                        });
                        nodes = parent_nodes;
                    }
                }
                XmlEvent::CData(characters)
                | XmlEvent::Characters(characters)
                | XmlEvent::Whitespace(characters) => {
                    if let Some(Node::Text(text)) = nodes.last_mut() {
                        text.push_str(&characters);
                    } else {
                        nodes.push(Node::Text(characters));
                    }
                }
                XmlEvent::EndDocument => break,
                _ => {}
            }
        }
        Ok(nodes)
    }
}

use xml_parser::{Node, XmlAttribute};

// ---------------------------------------------------------------------------
// Font list data structures
// ---------------------------------------------------------------------------

struct Font {
    filename: String,
    weight: Option<i32>,
    style: Option<String>,
}

struct FontFamily {
    name: String,
    fonts: Vec<Font>,
}

struct FontAlias {
    from: String,
    to: String,
    weight: Option<i32>,
}

struct FontList {
    families: Vec<FontFamily>,
    aliases: Vec<FontAlias>,
}

static FONT_LIST: LazyLock<FontList> = LazyLock::new(FontList::new);

impl FontList {
    fn new() -> FontList {
        let paths = [
            "/etc/fonts.xml",
            "/system/etc/system_fonts.xml",
            "/package/etc/fonts.xml",
        ];

        for path in &paths {
            if let Some(list) = Self::from_path(path) {
                return list;
            }
        }

        // Fallback when no XML is found.
        FontList {
            families: Self::fallback_font_families(),
            aliases: Vec::new(),
        }
    }

    fn from_path(path: &str) -> Option<FontList> {
        let bytes = std::fs::read(path).ok()?;
        let nodes = xml_parser::parse(&bytes).ok()?;

        let familyset = nodes.iter().find_map(|e| match e {
            Node::Element { name, children, .. } if name.local_name == "familyset" => {
                Some(children)
            }
            _ => None,
        })?;

        let mut families = Vec::new();
        let mut aliases = Vec::new();

        for node in familyset {
            if let Node::Element {
                name,
                attributes,
                children,
            } = node
            {
                if name.local_name == "family" {
                    Self::parse_family(children, attributes, &mut families);
                } else if name.local_name == "alias" && !families.is_empty() {
                    Self::parse_alias(attributes, &mut aliases);
                }
            }
        }

        Some(FontList { families, aliases })
    }

    fn fallback_font_families() -> Vec<FontFamily> {
        let alternatives = [
            ("sans-serif", "Roboto-Regular.ttf"),
            ("Droid Sans", "DroidSans.ttf"),
        ];

        alternatives
            .iter()
            .filter(|(_, file)| Path::new(&Self::font_absolute_path(file)).exists())
            .map(|(name, file)| FontFamily {
                name: (*name).into(),
                fonts: vec![Font {
                    filename: (*file).into(),
                    weight: None,
                    style: None,
                }],
            })
            .collect()
    }

    fn font_absolute_path(filename: &str) -> String {
        if filename.starts_with('/') {
            String::from(filename)
        } else {
            format!("/system/fonts/{}", filename)
        }
    }

    fn find_family(&self, name: &str) -> Option<&FontFamily> {
        self.families
            .iter()
            .find(|f| f.name.eq_ignore_ascii_case(name))
    }

    fn find_alias(&self, name: &str) -> Option<&FontAlias> {
        self.aliases
            .iter()
            .find(|a| a.from.eq_ignore_ascii_case(name))
    }

    fn parse_family(nodes: &[Node], attrs: &[XmlAttribute], out: &mut Vec<FontFamily>) {
        // Detect old API v17 format (nameset/fileset).
        let using_v17 = nodes.iter().any(|n| matches!(n, Node::Element { name, .. } if name.local_name == "nameset"));
        if using_v17 {
            Self::parse_family_v17(nodes, out);
            return;
        }

        let Some(name) = Self::find_attr("name", attrs) else {
            return;
        };

        let mut fonts = Vec::new();
        for node in nodes {
            if let Node::Element {
                name,
                attributes,
                children,
            } = node
            {
                if name.local_name == "font" {
                    Self::parse_font(children, attributes, &mut fonts);
                }
            }
        }

        out.push(FontFamily { name, fonts });
    }

    fn parse_family_v17(nodes: &[Node], out: &mut Vec<FontFamily>) {
        let mut names = Vec::new();
        let mut files = Vec::new();

        for node in nodes {
            if let Node::Element { name, children, .. } = node {
                if name.local_name == "nameset" {
                    Self::collect_tag_text(children, "name", &mut names);
                } else if name.local_name == "fileset" {
                    Self::collect_tag_text(children, "file", &mut files);
                }
            }
        }

        for name in names {
            let fonts: Vec<Font> = files
                .iter()
                .map(|f| Font {
                    filename: f.clone(),
                    weight: None,
                    style: None,
                })
                .collect();
            if !fonts.is_empty() {
                out.push(FontFamily { name, fonts });
            }
        }
    }

    fn parse_font(nodes: &[Node], attrs: &[XmlAttribute], out: &mut Vec<Font>) {
        if let Some(filename) = Self::text_content(nodes) {
            let weight = Self::find_attr("weight", attrs).and_then(|w| w.parse().ok());
            let style = Self::find_attr("style", attrs);
            out.push(Font {
                filename,
                weight,
                style,
            });
        }
    }

    fn parse_alias(attrs: &[XmlAttribute], out: &mut Vec<FontAlias>) {
        let Some(from) = Self::find_attr("name", attrs) else {
            return;
        };
        let Some(to) = Self::find_attr("to", attrs) else {
            return;
        };
        let weight = Self::find_attr("weight", attrs).and_then(|w| w.parse().ok());
        out.push(FontAlias { from, to, weight });
    }

    fn find_attr(name: &str, attrs: &[XmlAttribute]) -> Option<String> {
        attrs
            .iter()
            .find(|a| a.name.local_name == name)
            .map(|a| a.value.clone())
    }

    fn text_content(nodes: &[Node]) -> Option<String> {
        nodes.first().and_then(|n| match n {
            Node::Text(s) => Some(s.trim().into()),
            _ => None,
        })
    }

    fn collect_tag_text(nodes: &[Node], tag: &str, out: &mut Vec<String>) {
        for node in nodes {
            if let Node::Element { name, children, .. } = node {
                if name.local_name == tag {
                    if let Some(text) = Self::text_content(children) {
                        out.push(text);
                    }
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

pub fn system_font_families() -> Vec<String> {
    let mut families: Vec<String> = FONT_LIST
        .families
        .iter()
        .map(|f| f.name.clone())
        .chain(FONT_LIST.aliases.iter().map(|a| a.from.clone()))
        .collect();
    families.sort();
    families.dedup();
    families
}

pub fn system_font_faces(family: &str) -> Vec<SystemFontFace> {
    let mut faces = Vec::new();

    let mut add_font = |font: &Font| {
        let weight = font.weight.map(|w| w as f32).unwrap_or(400.0);
        let style = match font.style.as_deref() {
            Some("italic") => FontStyle::Italic,
            _ => FontStyle::Normal,
        };
        faces.push(SystemFontFace {
            path: PathBuf::from(FontList::font_absolute_path(&font.filename)),
            index: 0,
            weight,
            stretch: 1.0, // NORMAL
            style,
        });
    };

    if let Some(fam) = FONT_LIST.find_family(family) {
        for font in &fam.fonts {
            add_font(font);
        }
        return faces;
    }

    if let Some(alias) = FONT_LIST.find_alias(family) {
        if let Some(fam) = FONT_LIST.find_family(&alias.to) {
            for font in &fam.fonts {
                match (alias.weight, font.weight) {
                    (None, _) => add_font(font),
                    (Some(aw), Some(fw)) if aw == fw => add_font(font),
                    _ => {}
                }
            }
        }
    }

    faces
}

pub fn default_generic_family(generic: GenericFamily) -> String {
    match generic {
        GenericFamily::Serif => "serif",
        GenericFamily::SansSerif => "sans-serif",
        GenericFamily::Monospace => "monospace",
        GenericFamily::Cursive => "cursive",
        GenericFamily::Fantasy => "serif",
        GenericFamily::SystemUi => "Droid Sans",
    }
    .to_owned()
}

/// Check if a character is in a CJK Unicode block or supplementary plane 2/3.
fn is_cjk(ch: char) -> bool {
    let cp = ch as u32;

    // Supplementary Ideographic Plane (Plane 2) and Tertiary Ideographic Plane (Plane 3).
    if (0x20000..=0x3FFFF).contains(&cp) {
        return true;
    }

    if let Some(block) = ch.block() {
        matches!(
            block,
            UnicodeBlock::CJKUnifiedIdeographs
                | UnicodeBlock::CJKUnifiedIdeographsExtensionA
                | UnicodeBlock::CJKCompatibilityIdeographs
                | UnicodeBlock::CJKRadicalsSupplement
                | UnicodeBlock::KangxiRadicals
                | UnicodeBlock::IdeographicDescriptionCharacters
                | UnicodeBlock::CJKSymbolsandPunctuation
                | UnicodeBlock::CJKStrokes
                | UnicodeBlock::CJKCompatibility
                | UnicodeBlock::CJKCompatibilityForms
                | UnicodeBlock::EnclosedCJKLettersandMonths
                | UnicodeBlock::EnclosedIdeographicSupplement
                | UnicodeBlock::Bopomofo
                | UnicodeBlock::BopomofoExtended
                | UnicodeBlock::HalfwidthandFullwidthForms
                | UnicodeBlock::HangulCompatibilityJamo
                | UnicodeBlock::HangulJamo
                | UnicodeBlock::HangulJamoExtendedA
                | UnicodeBlock::HangulJamoExtendedB
                | UnicodeBlock::HangulSyllables
                | UnicodeBlock::Hiragana
                | UnicodeBlock::Katakana
                | UnicodeBlock::KatakanaPhoneticExtensions
                | UnicodeBlock::Kanbun
        )
    } else {
        false
    }
}

pub fn fallback_families(ch: char) -> Vec<&'static str> {
    let mut families = Vec::new();

    if let Some(block) = ch.block() {
        match block {
            UnicodeBlock::Armenian => families.push("Droid Sans Armenian"),
            UnicodeBlock::Hebrew => families.push("Droid Sans Hebrew"),
            UnicodeBlock::Arabic => families.push("Droid Sans Arabic"),
            UnicodeBlock::Devanagari => {
                families.push("Noto Sans Devanagari");
                families.push("Droid Sans Devanagari");
            }
            UnicodeBlock::Tamil => {
                families.push("Noto Sans Tamil");
                families.push("Droid Sans Tamil");
            }
            UnicodeBlock::Thai => {
                families.push("Noto Sans Thai");
                families.push("Droid Sans Thai");
            }
            UnicodeBlock::Georgian | UnicodeBlock::GeorgianSupplement => {
                families.push("Droid Sans Georgian");
            }
            UnicodeBlock::Ethiopic | UnicodeBlock::EthiopicSupplement => {
                families.push("Droid Sans Ethiopic");
            }
            UnicodeBlock::Bengali => families.push("Noto Sans Bengali"),
            UnicodeBlock::Gujarati => families.push("Noto Sans Gujarati"),
            UnicodeBlock::Gurmukhi => families.push("Noto Sans Gurmukhi"),
            UnicodeBlock::Oriya => families.push("Noto Sans Oriya"),
            UnicodeBlock::Kannada => families.push("Noto Sans Kannada"),
            UnicodeBlock::Telugu => families.push("Noto Sans Telugu"),
            UnicodeBlock::Malayalam => families.push("Noto Sans Malayalam"),
            UnicodeBlock::Sinhala => families.push("Noto Sans Sinhala"),
            UnicodeBlock::Lao => families.push("Noto Sans Lao"),
            UnicodeBlock::Tibetan => families.push("Noto Sans Tibetan"),
            _ => {
                if is_cjk(ch) {
                    families.push("MotoyaLMaru");
                    families.push("Noto Sans CJK JP");
                    families.push("Droid Sans Japanese");
                }
            }
        }
    }

    families.push("Droid Sans Fallback");
    families
}
