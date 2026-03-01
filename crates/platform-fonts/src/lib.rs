use std::path::PathBuf;

mod unicode_block;
use unicode_block::{UnicodeBlock, UnicodeBlockMethod};

/// Font style classification.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum FontStyle {
    Normal,
    Italic,
    Oblique(f32),
}

/// A discovered system font face.
#[derive(Clone, Debug)]
pub struct SystemFontFace {
    pub path: PathBuf,
    pub index: u32,
    pub weight: f32,
    pub stretch: f32,
    pub style: FontStyle,
}

/// Generic font family categories.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum GenericFamily {
    Serif,
    SansSerif,
    Monospace,
    Cursive,
    Fantasy,
    SystemUi,
}

#[cfg(target_os = "linux")]
mod linux;

#[cfg(target_os = "linux")]
pub use linux::{default_generic_family, system_font_faces, system_font_families};

#[cfg(target_os = "android")]
mod android;

#[cfg(target_os = "android")]
pub use android::{default_generic_family, fallback_families, system_font_faces, system_font_families};

#[cfg(any(target_os = "macos", target_os = "ios"))]
mod macos;

#[cfg(any(target_os = "macos", target_os = "ios"))]
pub use macos::{default_generic_family, fallback_families, system_font_faces, system_font_families};

#[cfg(target_os = "windows")]
mod windows;

#[cfg(target_os = "windows")]
pub use windows::{default_generic_family, fallback_families, system_font_faces, system_font_families};

#[cfg(not(any(target_os = "linux", target_os = "android", target_os = "macos", target_os = "ios", target_os = "windows")))]
pub fn system_font_families() -> Vec<String> {
    Vec::new()
}

#[cfg(not(any(target_os = "linux", target_os = "android", target_os = "macos", target_os = "ios", target_os = "windows")))]
pub fn system_font_faces(_family: &str) -> Vec<SystemFontFace> {
    Vec::new()
}

#[cfg(not(any(target_os = "linux", target_os = "android", target_os = "macos", target_os = "ios", target_os = "windows")))]
pub fn default_generic_family(_generic: GenericFamily) -> String {
    String::new()
}

/// Fallback families for a character (Linux and other platforms).
#[cfg(not(any(target_os = "android", target_os = "macos", target_os = "ios", target_os = "windows")))]
pub fn fallback_families(ch: char) -> Vec<&'static str> {
    let mut families = Vec::new();

    if is_emoji_presentation(ch) {
        families.push("Noto Color Emoji");
    }

    add_noto_fallback_families(ch, &mut families);

    if let Some(block) = ch.block() {
        match block {
            UnicodeBlock::HalfwidthandFullwidthForms
            | UnicodeBlock::EnclosedIdeographicSupplement => {
                families.push("WenQuanYi Micro Hei");
            }
            UnicodeBlock::Hiragana
            | UnicodeBlock::Katakana
            | UnicodeBlock::KatakanaPhoneticExtensions => {
                families.push("TakaoPGothic");
            }
            _ => {}
        }
    }

    families.push("DejaVu Serif");
    families.push("FreeSerif");
    families.push("DejaVu Sans");
    families.push("DejaVu Sans Mono");
    families.push("FreeSans");
    families.push("Symbola");
    families.push("Droid Sans Fallback");

    families
}

fn is_emoji_presentation(ch: char) -> bool {
    use unicode_properties::emoji::EmojiStatus;
    use unicode_properties::UnicodeEmoji;
    matches!(
        ch.emoji_status(),
        EmojiStatus::EmojiPresentation
            | EmojiStatus::EmojiPresentationAndModifierBase
            | EmojiStatus::EmojiPresentationAndEmojiComponent
            | EmojiStatus::EmojiPresentationAndModifierAndEmojiComponent
    )
}

fn add_noto_fallback_families(ch: char, families: &mut Vec<&'static str>) {
    let add_chinese_families = |families: &mut Vec<&str>| {
        families.push("Noto Sans CJK HK");
        families.push("Noto Sans CJK SC");
        families.push("Noto Sans CJK TC");
        families.push("Noto Sans HK");
        families.push("Noto Sans SC");
        families.push("Noto Sans TC");
    };

    match unicode_script::Script::from(ch) {
        unicode_script::Script::Common
        | unicode_script::Script::Inherited
        | unicode_script::Script::Latin
        | unicode_script::Script::Cyrillic
        | unicode_script::Script::Greek => {
            families.push("Noto Sans");
        }
        unicode_script::Script::Bopomofo | unicode_script::Script::Han => {
            add_chinese_families(families);
        }
        _ => {}
    }

    if let Some(block) = ch.block() {
        match block {
            UnicodeBlock::HalfwidthandFullwidthForms
            | UnicodeBlock::EnclosedIdeographicSupplement => add_chinese_families(families),
            UnicodeBlock::Adlam => families.push("Noto Sans Adlam"),
            UnicodeBlock::Ahom => families.push("Noto Serif Ahom"),
            UnicodeBlock::AnatolianHieroglyphs => families.push("Noto Sans AnatoHiero"),
            UnicodeBlock::Arabic
            | UnicodeBlock::ArabicExtendedA
            | UnicodeBlock::ArabicPresentationFormsA
            | UnicodeBlock::ArabicPresentationFormsB => {
                families.push("Noto Sans Arabic");
                families.push("Noto Naskh Arabic");
            }
            UnicodeBlock::ArabicMathematicalAlphabeticSymbols => {
                families.push("Noto Sans Math");
            }
            UnicodeBlock::Armenian => families.push("Noto Sans Armenian"),
            UnicodeBlock::Avestan => families.push("Noto Sans Avestan"),
            UnicodeBlock::Balinese => families.push("Noto Sans Balinese"),
            UnicodeBlock::Bamum | UnicodeBlock::BamumSupplement => {
                families.push("Noto Sans Bamum")
            }
            UnicodeBlock::BassaVah => families.push("Noto Sans Bassa Vah"),
            UnicodeBlock::Batak => families.push("Noto Sans Batak"),
            UnicodeBlock::Bengali => families.push("Noto Sans Bengali"),
            UnicodeBlock::Bhaiksuki => families.push("Noto Sans Bhaiksuki"),
            UnicodeBlock::Brahmi => families.push("Noto Sans Brahmi"),
            UnicodeBlock::BraillePatterns => {}
            UnicodeBlock::Buginese => families.push("Noto Sans Buginese"),
            UnicodeBlock::Buhid => families.push("Noto Sans Buhid"),
            UnicodeBlock::Carian => families.push("Noto Sans Carian"),
            UnicodeBlock::CaucasianAlbanian => families.push("Noto Sans Caucasian Albanian"),
            UnicodeBlock::Chakma => families.push("Noto Sans Chakma"),
            UnicodeBlock::Cham => families.push("Noto Sans Cham"),
            UnicodeBlock::Cherokee | UnicodeBlock::CherokeeSupplement => {
                families.push("Noto Sans Cherokee")
            }
            UnicodeBlock::Coptic => families.push("Noto Sans Coptic"),
            UnicodeBlock::Cuneiform | UnicodeBlock::CuneiformNumbersandPunctuation => {
                families.push("Noto Sans Cuneiform")
            }
            UnicodeBlock::CypriotSyllabary => families.push("Noto Sans Cypriot"),
            UnicodeBlock::Deseret => families.push("Noto Sans Deseret"),
            UnicodeBlock::Devanagari
            | UnicodeBlock::DevanagariExtended
            | UnicodeBlock::CommonIndicNumberForms => families.push("Noto Sans Devanagari"),
            UnicodeBlock::Duployan => families.push("Noto Sans Duployan"),
            UnicodeBlock::EgyptianHieroglyphs => families.push("Noto Sans Egyptian Hieroglyphs"),
            UnicodeBlock::Elbasan => families.push("Noto Sans Elbasan"),
            UnicodeBlock::Ethiopic
            | UnicodeBlock::EthiopicExtended
            | UnicodeBlock::EthiopicExtendedA
            | UnicodeBlock::EthiopicSupplement => families.push("Noto Sans Ethiopic"),
            UnicodeBlock::Georgian | UnicodeBlock::GeorgianSupplement => {
                families.push("Noto Sans Georgian")
            }
            UnicodeBlock::Glagolitic | UnicodeBlock::GlagoliticSupplement => {
                families.push("Noto Sans Glagolitic")
            }
            UnicodeBlock::Gothic => families.push("Noto Sans Gothic"),
            UnicodeBlock::Grantha => families.push("Noto Sans Grantha"),
            UnicodeBlock::Gujarati => families.push("Noto Sans Gujarati"),
            UnicodeBlock::Gurmukhi => families.push("Noto Sans Gurmukhi"),
            UnicodeBlock::HangulCompatibilityJamo
            | UnicodeBlock::HangulJamo
            | UnicodeBlock::HangulJamoExtendedA
            | UnicodeBlock::HangulJamoExtendedB
            | UnicodeBlock::HangulSyllables => {
                families.push("Noto Sans KR");
                families.push("Noto Sans CJK KR");
            }
            UnicodeBlock::HanifiRohingya => families.push("Noto Sans Hanifi Rohingya"),
            UnicodeBlock::Hanunoo => families.push("Noto Sans Hanunoo"),
            UnicodeBlock::Hatran => families.push("Noto Sans Hatran"),
            UnicodeBlock::Hebrew => families.push("Noto Sans Hebrew"),
            UnicodeBlock::Hiragana
            | UnicodeBlock::Katakana
            | UnicodeBlock::KatakanaPhoneticExtensions => {
                families.push("Noto Sans JP");
                families.push("Noto Sans CJK JP");
            }
            UnicodeBlock::ImperialAramaic => families.push("Noto Sans Imperial Aramaic"),
            UnicodeBlock::InscriptionalPahlavi => {
                families.push("Noto Sans Inscriptional Pahlavi")
            }
            UnicodeBlock::InscriptionalParthian => {
                families.push("Noto Sans Inscriptional Parthian")
            }
            UnicodeBlock::Javanese => families.push("Noto Sans Javanese"),
            UnicodeBlock::Kaithi => families.push("Noto Sans Kaithi"),
            UnicodeBlock::Kannada => families.push("Noto Sans Kannada"),
            UnicodeBlock::KayahLi => families.push("Noto Sans Kayah Li"),
            UnicodeBlock::Kharoshthi => families.push("Noto Sans Kharoshthi"),
            UnicodeBlock::Khmer | UnicodeBlock::KhmerSymbols => {
                families.push("Noto Sans Khmer")
            }
            UnicodeBlock::Khojki => families.push("Noto Sans Khojki"),
            UnicodeBlock::Khudawadi => families.push("Noto Sans Khudawadi"),
            UnicodeBlock::Lao => families.push("Noto Sans Lao"),
            UnicodeBlock::Lepcha => families.push("Noto Sans Lepcha"),
            UnicodeBlock::Limbu => families.push("Noto Sans Limbu"),
            UnicodeBlock::LinearA => families.push("Noto Sans Linear A"),
            UnicodeBlock::LinearBIdeograms | UnicodeBlock::LinearBSyllabary => {
                families.push("Noto Sans Linear B")
            }
            UnicodeBlock::Lisu => families.push("Noto Sans Lisu"),
            UnicodeBlock::Lycian => families.push("Noto Sans Lycian"),
            UnicodeBlock::Lydian => families.push("Noto Sans Lydian"),
            UnicodeBlock::Mahajani => families.push("Noto Sans Mahajani"),
            UnicodeBlock::Malayalam => families.push("Noto Sans Malayalam"),
            UnicodeBlock::Mandaic => families.push("Noto Sans Mandaic"),
            UnicodeBlock::Manichaean => families.push("Noto Sans Manichaean"),
            UnicodeBlock::Marchen => families.push("Noto Sans Marchen"),
            UnicodeBlock::MeeteiMayek | UnicodeBlock::MeeteiMayekExtensions => {
                families.push("Noto Sans Meetei Mayek")
            }
            UnicodeBlock::MendeKikakui => families.push("Noto Sans Mende Kikakui"),
            UnicodeBlock::MeroiticCursive | UnicodeBlock::MeroiticHieroglyphs => {
                families.push("Noto Sans Meroitic")
            }
            UnicodeBlock::Miao => families.push("Noto Sans Miao"),
            UnicodeBlock::Modi => families.push("Noto Sans Modi"),
            UnicodeBlock::Mongolian | UnicodeBlock::MongolianSupplement => {
                families.push("Noto Sans Mongolian")
            }
            UnicodeBlock::Mro => families.push("Noto Sans Mro"),
            UnicodeBlock::Multani => families.push("Noto Sans Multani"),
            UnicodeBlock::MusicalSymbols => families.push("Noto Music"),
            UnicodeBlock::Myanmar | UnicodeBlock::MyanmarExtendedA | UnicodeBlock::MyanmarExtendedB => {
                families.push("Noto Sans Myanmar")
            }
            UnicodeBlock::NKo => families.push("Noto Sans NKo"),
            UnicodeBlock::Nabataean => families.push("Noto Sans Nabataean"),
            UnicodeBlock::NewTaiLue => families.push("Noto Sans New Tai Lue"),
            UnicodeBlock::Newa => families.push("Noto Sans Newa"),
            UnicodeBlock::Ogham => families.push("Noto Sans Ogham"),
            UnicodeBlock::OlChiki => families.push("Noto Sans Ol Chiki"),
            UnicodeBlock::OldHungarian => families.push("Noto Sans Old Hungarian"),
            UnicodeBlock::OldItalic => families.push("Noto Sans Old Italic"),
            UnicodeBlock::OldNorthArabian => families.push("Noto Sans Old North Arabian"),
            UnicodeBlock::OldPermic => families.push("Noto Sans Old Permic"),
            UnicodeBlock::OldPersian => families.push("Noto Sans Old Persian"),
            UnicodeBlock::OldSouthArabian => families.push("Noto Sans Old South Arabian"),
            UnicodeBlock::OldTurkic => families.push("Noto Sans Old Turkic"),
            UnicodeBlock::Oriya => families.push("Noto Sans Oriya"),
            UnicodeBlock::Osage => families.push("Noto Sans Osage"),
            UnicodeBlock::Osmanya => families.push("Noto Sans Osmanya"),
            UnicodeBlock::PahawhHmong => families.push("Noto Sans Pahawh Hmong"),
            UnicodeBlock::Palmyrene => families.push("Noto Sans Palmyrene"),
            UnicodeBlock::PauCinHau => families.push("Noto Sans Pau Cin Hau"),
            UnicodeBlock::Phagspa => families.push("Noto Sans PhagsPa"),
            UnicodeBlock::Phoenician => families.push("Noto Sans Phoenician"),
            UnicodeBlock::PsalterPahlavi => families.push("Noto Sans Psalter Pahlavi"),
            UnicodeBlock::Rejang => families.push("Noto Sans Rejang"),
            UnicodeBlock::Runic => families.push("Noto Sans Runic"),
            UnicodeBlock::Samaritan => families.push("Noto Sans Samaritan"),
            UnicodeBlock::Saurashtra => families.push("Noto Sans Saurashtra"),
            UnicodeBlock::Sharada => families.push("Noto Sans Sharada"),
            UnicodeBlock::Shavian => families.push("Noto Sans Shavian"),
            UnicodeBlock::Siddham => families.push("Noto Sans Siddham"),
            UnicodeBlock::Sinhala | UnicodeBlock::SinhalaArchaicNumbers => {
                families.push("Noto Sans Sinhala")
            }
            UnicodeBlock::SoraSompeng => families.push("Noto Sans Sora Sompeng"),
            UnicodeBlock::Sundanese => families.push("Noto Sans Sundanese"),
            UnicodeBlock::SuttonSignWriting => families.push("Noto Sans SignWrit"),
            UnicodeBlock::SylotiNagri => families.push("Noto Sans Syloti Nagri"),
            UnicodeBlock::Syriac => families.push("Noto Sans Syriac"),
            UnicodeBlock::Tagalog => families.push("Noto Sans Tagalog"),
            UnicodeBlock::Tagbanwa => families.push("Noto Sans Tagbanwa"),
            UnicodeBlock::TaiLe => families.push("Noto Sans Tai Le"),
            UnicodeBlock::TaiTham => families.push("Noto Sans Tai Tham"),
            UnicodeBlock::TaiViet => families.push("Noto Sans Tai Viet"),
            UnicodeBlock::Takri => families.push("Noto Sans Takri"),
            UnicodeBlock::Tamil => families.push("Noto Sans Tamil"),
            UnicodeBlock::Tangut
            | UnicodeBlock::TangutComponents
            | UnicodeBlock::IdeographicSymbolsandPunctuation => {
                families.push("Noto Serif Tangut")
            }
            UnicodeBlock::Telugu => families.push("Noto Sans Telugu"),
            UnicodeBlock::Thaana => families.push("Noto Sans Thaana"),
            UnicodeBlock::Thai => families.push("Noto Sans Thai"),
            UnicodeBlock::Tibetan => families.push("Noto Serif Tibetan"),
            UnicodeBlock::Tifinagh => families.push("Noto Sans Tifinagh"),
            UnicodeBlock::Tirhuta => families.push("Noto Sans Tirhuta"),
            UnicodeBlock::Ugaritic => families.push("Noto Sans Ugaritic"),
            UnicodeBlock::UnifiedCanadianAboriginalSyllabics
            | UnicodeBlock::UnifiedCanadianAboriginalSyllabicsExtended => {
                families.push("Noto Sans Canadian Aboriginal")
            }
            UnicodeBlock::Vai => families.push("Noto Sans Vai"),
            UnicodeBlock::WarangCiti => families.push("Noto Sans Warang Citi"),
            UnicodeBlock::YiSyllables | UnicodeBlock::YiRadicals => {
                families.push("Noto Sans Yi")
            }
            UnicodeBlock::Wancho => families.push("Noto Sans Wancho"),
            _ => {}
        }
    }

    families.push("Noto Sans Symbols");
    families.push("Noto Sans Symbols2");
}
