//! Метаданные файла шрифта: семейство и начертания (спека подсистемы 3, раздел 4).
//! Начертание статического шрифта — его подсемейство; у вариативного — каждый
//! именованный экземпляр (`fvar`), как их показывает Figma. Один формат описания
//! у манифеста вшитых шрифтов, системных шрифтов Chrome и десктопа.

use serde::{Deserialize, Serialize};
use skrifa::raw::types::NameId;
use skrifa::raw::{FileRef, TableProvider};
use skrifa::{FontRef, MetadataProvider, Tag};

/// Наибольший файл шрифта (спека 4): больше — не читается.
pub const MAX_FONT_FILE: usize = 64 * 1024 * 1024;

/// Начертание: имя («Semi Bold Italic»), вес 1–1000, курсив, ширина (1 — обычная)
/// и где его взять — файл и номер шрифта в коллекции (`.ttc`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FaceInfo {
    pub style: String,
    pub weight: f32,
    pub italic: bool,
    pub stretch: f32,
    pub file: String,
    #[serde(default)]
    pub index: u32,
}

/// Семейство и его начертания.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FamilyInfo {
    pub family: String,
    pub faces: Vec<FaceInfo>,
}

fn name(font: &FontRef, id: NameId) -> Option<String> {
    let s: String = font
        .localized_strings(id)
        .english_or_first()?
        .chars()
        .collect();
    let s = s.trim().to_string();
    (!s.is_empty()).then_some(s)
}

/// Начертания всех шрифтов файла (у коллекции — каждого) с именем семейства.
/// Неразборчивый файл — пустой список (вызывающий пишет сообщение в журнал).
pub fn read_faces(bytes: &[u8], file: &str) -> Vec<(String, FaceInfo)> {
    if bytes.len() > MAX_FONT_FILE {
        return Vec::new();
    }
    let fonts: Vec<(u32, FontRef)> = match FileRef::new(bytes) {
        Ok(FileRef::Font(f)) => vec![(0, f)],
        Ok(FileRef::Collection(c)) => c
            .iter()
            .enumerate()
            .filter_map(|(i, f)| Some((i as u32, f.ok()?)))
            .collect(),
        Err(_) => Vec::new(),
    };
    let mut out = Vec::new();
    for (index, font) in fonts {
        let Some(family) = name(&font, NameId::TYPOGRAPHIC_FAMILY_NAME)
            .or_else(|| name(&font, NameId::FAMILY_NAME))
        else {
            continue;
        };
        let attrs = font.attributes();
        let italic_file = !matches!(attrs.style, skrifa::attribute::Style::Normal);
        let axes = font.axes();
        let instances = font.named_instances();
        if instances.is_empty() {
            let style = name(&font, NameId::TYPOGRAPHIC_SUBFAMILY_NAME)
                .or_else(|| name(&font, NameId::SUBFAMILY_NAME))
                .unwrap_or_else(|| "Regular".to_string());
            out.push((
                family,
                FaceInfo {
                    style,
                    weight: attrs.weight.value(),
                    italic: italic_file,
                    stretch: attrs.stretch.ratio(),
                    file: file.to_string(),
                    index,
                },
            ));
            continue;
        }
        for inst in instances.iter() {
            let Some(style) = name(&font, inst.subfamily_name_id()) else {
                continue;
            };
            let coord = |tag: &[u8; 4]| {
                let axis = axes.get_by_tag(Tag::new(tag))?;
                inst.user_coords().nth(axis.index())
            };
            out.push((
                family.clone(),
                FaceInfo {
                    style,
                    weight: coord(b"wght").unwrap_or(attrs.weight.value()),
                    italic: italic_file || coord(b"ital").is_some_and(|v| v >= 1.0),
                    stretch: coord(b"wdth").map_or(attrs.stretch.ratio(), |w| w / 100.0),
                    file: file.to_string(),
                    index,
                },
            ));
        }
    }
    // Таблица `head` есть у любого разборчивого шрифта: без неё — мусор.
    out.retain(|_| FontRef::new(bytes).map_or(true, |f| f.head().is_ok()));
    out
}

/// Семейства из начертаний в порядке первого появления; повторы (то же имя
/// начертания в двух файлах) отбрасываются.
pub fn group(faces: impl IntoIterator<Item = (String, FaceInfo)>) -> Vec<FamilyInfo> {
    let mut out: Vec<FamilyInfo> = Vec::new();
    for (family, face) in faces {
        let entry = match out.iter_mut().position(|f| f.family == family) {
            Some(i) => &mut out[i],
            None => {
                out.push(FamilyInfo {
                    family,
                    faces: Vec::new(),
                });
                out.last_mut().expect("только что добавлено")
            }
        };
        if !entry.faces.iter().any(|f| f.style == face.style) {
            entry.faces.push(face);
        }
    }
    out
}
