use std::path::PathBuf;

use dwrote::{Font, FontCollection, FontStretch, FontStyle as DWriteFontStyle};

use crate::unicode_block::{UnicodeBlock, UnicodeBlockMethod};
use crate::{FontStyle, GenericFamily, SystemFontFace};

fn unicode_plane(ch: char) -> u32 {
    (ch as u32) >> 16
}

fn stretch_from_dwrote(stretch: FontStretch) -> f32 {
    match stretch {
        FontStretch::UltraCondensed => 0.5,
        FontStretch::ExtraCondensed => 0.625,
        FontStretch::Condensed => 0.75,
        FontStretch::SemiCondensed => 0.875,
        FontStretch::Normal | FontStretch::Undefined => 1.0,
        FontStretch::SemiExpanded => 1.125,
        FontStretch::Expanded => 1.25,
        FontStretch::ExtraExpanded => 1.5,
        FontStretch::UltraExpanded => 2.0,
    }
}

fn style_from_dwrote(style: DWriteFontStyle) -> FontStyle {
    match style {
        DWriteFontStyle::Normal => FontStyle::Normal,
        DWriteFontStyle::Italic => FontStyle::Italic,
        DWriteFontStyle::Oblique => FontStyle::Oblique(14.0),
    }
}

fn face_from_font(font: &Font) -> Option<SystemFontFace> {
    let face = font.create_font_face();
    let files = face.get_files();
    let file = files.first()?;
    let path = file.get_font_file_path()?;

    Some(SystemFontFace {
        path,
        index: face.get_index(),
        weight: font.weight().to_u32() as f32,
        stretch: stretch_from_dwrote(font.stretch()),
        style: style_from_dwrote(font.style()),
    })
}

pub fn system_font_families() -> Vec<String> {
    let system_fc = FontCollection::system();
    let mut families = Vec::new();
    for family in system_fc.families_iter() {
        if let Ok(name) = family.family_name() {
            families.push(name);
        }
    }
    families.sort();
    families.dedup();
    families
}

pub fn system_font_faces(family: &str) -> Vec<SystemFontFace> {
    let system_fc = FontCollection::system();
    let Ok(Some(family)) = system_fc.font_family_by_name(family) else {
        return Vec::new();
    };
    let mut faces = Vec::new();
    let count = family.get_font_count();
    for i in 0..count {
        let Ok(font) = family.font(i) else {
            continue;
        };
        if let Some(face) = face_from_font(&font) {
            faces.push(face);
        }
    }
    faces
}

pub fn default_generic_family(generic: GenericFamily) -> String {
    match generic {
        GenericFamily::Serif => "Times New Roman",
        GenericFamily::SansSerif => "Arial",
        GenericFamily::Monospace => "Courier New",
        GenericFamily::Cursive => "Comic Sans MS",
        GenericFamily::Fantasy => "Impact",
        GenericFamily::SystemUi => "Segoe UI",
    }
    .to_owned()
}

pub fn fallback_families(ch: char) -> Vec<&'static str> {
    let mut families = Vec::new();

    if is_emoji_presentation(ch) {
        families.push("Segoe UI Emoji");
    }

    families.push("Arial");

    match unicode_plane(ch) {
        // Basic Multilingual Plane
        0 => {
            if let Some(block) = ch.block() {
                match block {
                    UnicodeBlock::CyrillicSupplement
                    | UnicodeBlock::Armenian
                    | UnicodeBlock::Hebrew => {
                        families.push("Estrangelo Edessa");
                        families.push("Cambria");
                    }

                    UnicodeBlock::Arabic | UnicodeBlock::ArabicSupplement => {
                        families.push("Microsoft Uighur");
                    }

                    UnicodeBlock::Syriac => {
                        families.push("Estrangelo Edessa");
                    }

                    UnicodeBlock::Thaana => {
                        families.push("MV Boli");
                    }

                    UnicodeBlock::NKo => {
                        families.push("Ebrima");
                    }

                    UnicodeBlock::Devanagari | UnicodeBlock::Bengali => {
                        families.push("Nirmala UI");
                        families.push("Utsaah");
                        families.push("Aparajita");
                    }

                    UnicodeBlock::Gurmukhi
                    | UnicodeBlock::Gujarati
                    | UnicodeBlock::Oriya
                    | UnicodeBlock::Tamil
                    | UnicodeBlock::Telugu
                    | UnicodeBlock::Kannada
                    | UnicodeBlock::Malayalam
                    | UnicodeBlock::Sinhala
                    | UnicodeBlock::Lepcha
                    | UnicodeBlock::OlChiki
                    | UnicodeBlock::CyrillicExtendedC
                    | UnicodeBlock::SundaneseSupplement
                    | UnicodeBlock::VedicExtensions => {
                        families.push("Nirmala UI");
                    }

                    UnicodeBlock::Thai => {
                        families.push("Leelawadee UI");
                    }

                    UnicodeBlock::Lao => {
                        families.push("Lao UI");
                    }

                    UnicodeBlock::Myanmar
                    | UnicodeBlock::MyanmarExtendedA
                    | UnicodeBlock::MyanmarExtendedB => {
                        families.push("Myanmar Text");
                    }

                    UnicodeBlock::HangulJamo
                    | UnicodeBlock::HangulJamoExtendedA
                    | UnicodeBlock::HangulSyllables
                    | UnicodeBlock::HangulJamoExtendedB
                    | UnicodeBlock::HangulCompatibilityJamo => {
                        families.push("Malgun Gothic");
                    }

                    UnicodeBlock::Ethiopic
                    | UnicodeBlock::EthiopicSupplement
                    | UnicodeBlock::EthiopicExtended
                    | UnicodeBlock::EthiopicExtendedA => {
                        families.push("Nyala");
                    }

                    UnicodeBlock::Cherokee => {
                        families.push("Plantagenet Cherokee");
                    }

                    UnicodeBlock::UnifiedCanadianAboriginalSyllabics
                    | UnicodeBlock::UnifiedCanadianAboriginalSyllabicsExtended => {
                        families.push("Euphemia");
                        families.push("Segoe UI");
                    }

                    UnicodeBlock::Khmer | UnicodeBlock::KhmerSymbols => {
                        families.push("Khmer UI");
                        families.push("Leelawadee UI");
                    }

                    UnicodeBlock::Mongolian => {
                        families.push("Mongolian Baiti");
                    }

                    UnicodeBlock::TaiLe => {
                        families.push("Microsoft Tai Le");
                    }

                    UnicodeBlock::NewTaiLue => {
                        families.push("Microsoft New Tai Lue");
                    }

                    UnicodeBlock::Buginese
                    | UnicodeBlock::TaiTham
                    | UnicodeBlock::CombiningDiacriticalMarksExtended => {
                        families.push("Leelawadee UI");
                    }

                    UnicodeBlock::GeneralPunctuation
                    | UnicodeBlock::SuperscriptsandSubscripts
                    | UnicodeBlock::CurrencySymbols
                    | UnicodeBlock::CombiningDiacriticalMarksforSymbols
                    | UnicodeBlock::LetterlikeSymbols
                    | UnicodeBlock::NumberForms
                    | UnicodeBlock::Arrows
                    | UnicodeBlock::MathematicalOperators
                    | UnicodeBlock::MiscellaneousTechnical
                    | UnicodeBlock::ControlPictures
                    | UnicodeBlock::OpticalCharacterRecognition
                    | UnicodeBlock::EnclosedAlphanumerics
                    | UnicodeBlock::BoxDrawing
                    | UnicodeBlock::BlockElements
                    | UnicodeBlock::GeometricShapes
                    | UnicodeBlock::MiscellaneousSymbols
                    | UnicodeBlock::Dingbats
                    | UnicodeBlock::MiscellaneousMathematicalSymbolsA
                    | UnicodeBlock::SupplementalArrowsA
                    | UnicodeBlock::SupplementalArrowsB
                    | UnicodeBlock::MiscellaneousMathematicalSymbolsB
                    | UnicodeBlock::SupplementalMathematicalOperators
                    | UnicodeBlock::MiscellaneousSymbolsandArrows
                    | UnicodeBlock::Glagolitic
                    | UnicodeBlock::LatinExtendedC
                    | UnicodeBlock::Coptic => {
                        families.push("Segoe UI");
                        families.push("Segoe UI Symbol");
                        families.push("Cambria");
                        families.push("Meiryo");
                        families.push("Lucida Sans Unicode");
                        families.push("Ebrima");
                    }

                    UnicodeBlock::GeorgianSupplement
                    | UnicodeBlock::Tifinagh
                    | UnicodeBlock::CyrillicExtendedA
                    | UnicodeBlock::SupplementalPunctuation
                    | UnicodeBlock::CJKRadicalsSupplement
                    | UnicodeBlock::KangxiRadicals
                    | UnicodeBlock::IdeographicDescriptionCharacters => {
                        families.push("Segoe UI");
                        families.push("Segoe UI Symbol");
                        families.push("Meiryo");
                    }

                    UnicodeBlock::BraillePatterns => {
                        families.push("Segoe UI Symbol");
                    }

                    UnicodeBlock::CJKSymbolsandPunctuation
                    | UnicodeBlock::Hiragana
                    | UnicodeBlock::Katakana
                    | UnicodeBlock::Bopomofo
                    | UnicodeBlock::Kanbun
                    | UnicodeBlock::BopomofoExtended
                    | UnicodeBlock::CJKStrokes
                    | UnicodeBlock::KatakanaPhoneticExtensions
                    | UnicodeBlock::CJKUnifiedIdeographs => {
                        families.push("Microsoft YaHei");
                        families.push("Yu Gothic");
                    }

                    UnicodeBlock::EnclosedCJKLettersandMonths => {
                        families.push("Malgun Gothic");
                    }

                    UnicodeBlock::YijingHexagramSymbols => {
                        families.push("Segoe UI Symbol");
                    }

                    UnicodeBlock::YiSyllables | UnicodeBlock::YiRadicals => {
                        families.push("Microsoft Yi Baiti");
                        families.push("Segoe UI");
                    }

                    UnicodeBlock::Vai
                    | UnicodeBlock::CyrillicExtendedB
                    | UnicodeBlock::Bamum
                    | UnicodeBlock::ModifierToneLetters
                    | UnicodeBlock::LatinExtendedD => {
                        families.push("Ebrima");
                        families.push("Segoe UI");
                        families.push("Cambria Math");
                    }

                    UnicodeBlock::SylotiNagri
                    | UnicodeBlock::CommonIndicNumberForms
                    | UnicodeBlock::Phagspa
                    | UnicodeBlock::Saurashtra
                    | UnicodeBlock::DevanagariExtended => {
                        families.push("Microsoft PhagsPa");
                        families.push("Nirmala UI");
                    }

                    UnicodeBlock::KayahLi | UnicodeBlock::Rejang | UnicodeBlock::Javanese => {
                        families.push("Malgun Gothic");
                        families.push("Javanese Text");
                        families.push("Leelawadee UI");
                    }

                    UnicodeBlock::AlphabeticPresentationForms => {
                        families.push("Microsoft Uighur");
                        families.push("Gabriola");
                        families.push("Sylfaen");
                    }

                    UnicodeBlock::ArabicPresentationFormsA
                    | UnicodeBlock::ArabicPresentationFormsB => {
                        families.push("Traditional Arabic");
                        families.push("Arabic Typesetting");
                    }

                    UnicodeBlock::VariationSelectors
                    | UnicodeBlock::VerticalForms
                    | UnicodeBlock::CombiningHalfMarks
                    | UnicodeBlock::CJKCompatibilityForms
                    | UnicodeBlock::SmallFormVariants
                    | UnicodeBlock::HalfwidthandFullwidthForms
                    | UnicodeBlock::Specials => {
                        families.push("Microsoft JhengHei");
                    }

                    _ => {}
                }
            }
        }

        // Supplementary Multilingual Plane
        1 => {
            families.push("Segoe UI Symbol");
            families.push("Ebrima");
            families.push("Nirmala UI");
            families.push("Cambria Math");
        }

        _ => {}
    }

    families.push("Arial Unicode MS");
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
