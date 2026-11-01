//! Экспорт PDF через `krilla` (спека подсистемы 3, раздел 11): фрейм — страница
//! своего размера, текст — настоящим текстом со встроенным подмножеством шрифта,
//! картинки — изображениями PDF, пределы. Файл тест читает как есть: словари
//! страниц и шрифтов — открытым текстом, потоки — после распаковки Deflate.

mod common;

use std::cell::Cell;
use std::collections::HashMap;
use std::io::Read;
use std::path::PathBuf;
use std::sync::Arc;

use common::*;
use ferrum_doc::*;
use ferrum_render::{
    decode_png, Clock, ExportBudget, ExportError, Fonts, NoImages, Pixmap, Renderer, SharedFonts,
    MAX_PDF_SIDE,
};

fn solid(r: u8, g: u8, b: u8) -> Option<Vec<Fill>> {
    Some(vec![Fill::solid(Color::rgba(r, g, b, 255))])
}

fn pdf(r: &mut Renderer, p: &mut Page, pages: &[NodeId]) -> Vec<u8> {
    let scene = p.scene();
    r.export_pdf(&scene, pages, &NoImages, &ExportBudget::unlimited())
        .unwrap()
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

/// Все потоки файла, распакованные (несжатые — как есть).
fn streams(bytes: &[u8]) -> Vec<Vec<u8>> {
    let mut out = Vec::new();
    let mut at = 0;
    while let Some(i) = find(&bytes[at..], b"stream") {
        let start = at + i + b"stream".len();
        // «endstream» тоже содержит «stream»: пропускаем.
        if bytes[..at + i].ends_with(b"end") {
            at = start;
            continue;
        }
        let body = start + if bytes[start] == b'\r' { 2 } else { 1 };
        let Some(end) = find(&bytes[body..], b"endstream") else {
            break;
        };
        let raw = &bytes[body..body + end];
        let mut inflated = Vec::new();
        if flate2::read::ZlibDecoder::new(raw)
            .read_to_end(&mut inflated)
            .is_ok()
        {
            out.push(inflated);
        } else {
            out.push(raw.to_vec());
        }
        at = body + end + b"endstream".len();
    }
    out
}

fn find(hay: &[u8], needle: &[u8]) -> Option<usize> {
    hay.windows(needle.len()).position(|w| w == needle)
}

/// Размеры страниц по `/MediaBox[0 0 w h]` (`pdf-writer` пишет без пробелов).
fn media_boxes(bytes: &[u8]) -> Vec<(f64, f64)> {
    let s = text(bytes);
    s.match_indices("/MediaBox[")
        .map(|(i, m)| {
            let rest = &s[i + m.len()..];
            let nums: Vec<f64> = rest[..rest.find(']').unwrap()]
                .split_whitespace()
                .map(|v| v.parse().unwrap())
                .collect();
            (nums[2] - nums[0], nums[3] - nums[1])
        })
        .collect()
}

#[test]
fn every_frame_is_a_page_of_its_own_size() {
    let mut p = Page::new();
    let a = p.add(
        None,
        NodeKind::Frame,
        PropsPatch {
            fills: solid(255, 255, 255),
            ..rect(0.0, 0.0, 200.0, 120.0)
        },
    );
    p.add(
        Some(a),
        NodeKind::Rect,
        PropsPatch {
            fills: solid(255, 0, 0),
            corner_radius: Some(8.0),
            strokes: Some(vec![Stroke {
                color: Color::BLACK,
                width: 2.0,
                align: StrokeAlign::Inside,
            }]),
            ..rect(150.0, 10.0, 80.0, 40.0)
        },
    );
    let b = p.add(
        None,
        NodeKind::Frame,
        PropsPatch {
            opacity: Some(0.5),
            fills: solid(0, 0, 255),
            ..rect(300.0, 0.0, 393.0, 852.0)
        },
    );
    let bytes = pdf(&mut Renderer::new(), &mut p, &[a, b]);
    assert!(bytes.starts_with(b"%PDF-"));
    assert!(text(&bytes).trim_end().ends_with("%%EOF"));
    assert_eq!(media_boxes(&bytes), [(200.0, 120.0), (393.0, 852.0)]);
    let content: Vec<String> = streams(&bytes).iter().map(|s| text(s)).collect();
    // Обрезка фрейма (`W n`), красная заливка, группа прозрачности второй страницы.
    assert!(content.iter().any(|c| c.contains("W\nn")), "обрезка фрейма");
    assert!(
        content.iter().any(|c| c.contains("1 0 0 rg")),
        "красная заливка"
    );
    assert!(text(&bytes).contains("/ca 0.5"), "прозрачность фрейма");
    // Ни одной картинки: всё — векторами.
    assert!(!text(&bytes).contains("/Subtype/Image"));
}

fn font_file(rel: &str) -> Vec<u8> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../apps/web/public/fonts")
        .join(rel);
    std::fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

fn inter() -> SharedFonts {
    family(
        "Inter",
        &[
            "inter/Inter-Variable.ttf",
            "inter/Inter-Italic-Variable.ttf",
        ],
    )
}

/// Семейство из файлов каталога шрифтов приложения.
fn family(name: &str, files: &[&str]) -> SharedFonts {
    let files: Vec<_> = files.iter().map(|r| (*r, font_file(r))).collect();
    family_with(name, &files, &|bytes| bytes)
}

/// Семейство из байтов файлов (имя файла, содержимое); `patch` меняет байты
/// только у загружаемого шрифта, а список шрифтов строится по исходным.
fn family_with(
    name: &str,
    files: &[(&str, Vec<u8>)],
    patch: &dyn Fn(Vec<u8>) -> Vec<u8>,
) -> SharedFonts {
    let shared = Fonts::shared();
    {
        let mut f = shared.borrow_mut();
        let faces = files
            .iter()
            .flat_map(|(r, bytes)| ferrum_text::read_faces(bytes, r))
            .collect::<Vec<_>>();
        f.add_catalog(ferrum_text::SourceKind::Bundled, ferrum_text::group(faces));
        for (r, bytes) in files {
            f.load(name, r, patch(bytes.clone())).unwrap();
        }
    }
    shared
}

fn add_text(p: &mut Page, s: &str, style: &str, fills: Vec<Fill>, y: f64) -> NodeId {
    add_text_in(p, "Inter", s, style, fills, y)
}

fn add_text_in(
    p: &mut Page,
    family: &str,
    s: &str,
    style: &str,
    fills: Vec<Fill>,
    y: f64,
) -> NodeId {
    let mut props = TextProps::default();
    props.style.size = 24.0;
    props.style.font = FontName::new(family, style);
    p.doc
        .transact(p.page, vec![], |tx| {
            let id = tx.create_node(None, None, NodeKind::Text)?;
            tx.set_props(
                id,
                &PropsPatch {
                    text: Some(props.clone()),
                    fills: Some(fills.clone()),
                    ..rect(0.0, y, 400.0, 40.0)
                },
            )?;
            tx.set_text_content(id, &TextContent::plain(s))?;
            Ok(vec![id])
        })
        .unwrap()[0]
}

#[test]
fn text_is_real_text_with_font_subsets() {
    let mut p = Page::new();
    let black = vec![Fill::solid(Color::BLACK)];
    let regular = add_text(&mut p, "Привет, PDF", "Regular", black.clone(), 0.0);
    let bold = add_text(&mut p, "Жирный", "Bold", black, 50.0);
    let mut r = Renderer::new();
    r.set_fonts(inter());
    let bytes = pdf(&mut r, &mut p, &[regular, bold]);
    let raw = text(&bytes);
    // Шрифт встроен подмножеством (имя с префиксом «ABCDEF+»), по шрифту на вес.
    assert!(raw.contains("/FontFile2") || raw.contains("/FontFile3"));
    // Два веса — два разных встроенных шрифта (имена подмножеств различаются).
    let mut names: Vec<&str> = raw
        .match_indices("/BaseFont/")
        .map(|(i, m)| {
            let rest = &raw[i + m.len()..];
            &rest[..rest
                .find(|c: char| !(c.is_ascii_alphanumeric() || c == '+' || c == '-'))
                .unwrap()]
        })
        .collect();
    names.sort_unstable();
    names.dedup();
    assert_eq!(names.len(), 2, "{names:?}");
    assert!(names.iter().all(|n| n.contains("+Inter")), "{names:?}");
    assert_eq!(raw.matches("/FontFile2").count(), 2, "по файлу на вес");
    let content: Vec<String> = streams(&bytes).iter().map(|s| text(s)).collect();
    assert!(
        content.iter().any(|c| c.contains("Tj") || c.contains("TJ")),
        "текстовые операторы"
    );
    // Текст для поиска и копирования — ToUnicode каждого из двух шрифтов: у
    // обычного есть «П» (U+041F), у жирного — «Ж», и не наоборот.
    let maps = to_unicode(&bytes);
    assert_eq!(maps.len(), 2);
    let has = |m: &HashMap<u16, String>, c: &str| m.values().any(|v| v == c);
    assert!(maps.iter().any(|m| has(m, "П") && !has(m, "Ж")));
    assert!(maps.iter().any(|m| has(m, "Ж") && !has(m, "П")));
    // Кривыми текст не нарисован: контуров глифов (`c` и `f`) почти нет.
    assert!(!content.iter().any(|c| c.matches(" c\n").count() > 50));
}

#[test]
fn image_fills_become_pdf_images_drawn_once_per_tile() {
    let mut p = Page::new();
    let mut px = Pixmap::new(2, 1);
    px.data_as_u8_slice_mut()
        .copy_from_slice(&[255, 0, 0, 255, 0, 0, 255, 255]);
    let png = px.into_png().unwrap();
    let hash = p.doc.images.add(png.clone());
    let id = p.add(
        None,
        NodeKind::Rect,
        PropsPatch {
            fills: Some(vec![Fill {
                scale_mode: ScaleMode::Tile,
                ..Fill::image(hash.clone())
            }]),
            ..rect(0.0, 0.0, 40.0, 40.0)
        },
    );
    let scene = p.scene();
    let images: HashMap<String, Arc<Pixmap>> =
        HashMap::from([(hash, Arc::new(decode_png(&png).unwrap()))]);
    let bytes = Renderer::new()
        .export_pdf(&scene, &[id], &images, &ExportBudget::unlimited())
        .unwrap();
    let raw = text(&bytes);
    assert_eq!(raw.matches("/Subtype/Image").count(), 1, "картинка — одна");
    assert!(raw.contains("/Width 2") && raw.contains("/Height 1"));
    // Плитки 2×1 на 40×40 — 20 × 40 рисований одной картинки.
    let content: Vec<String> = streams(&bytes).iter().map(|s| text(s)).collect();
    let draws: usize = content.iter().map(|c| c.matches(" Do").count()).sum();
    assert_eq!(draws, 800);
}

#[test]
fn huge_frames_shrink_to_the_page_limit_and_errors_are_reported() {
    let mut p = Page::new();
    let big = p.add(
        None,
        NodeKind::Frame,
        PropsPatch {
            fills: solid(0, 0, 0),
            ..rect(0.0, 0.0, 30_000.0, 300.0)
        },
    );
    let hidden = p.add(
        None,
        NodeKind::Rect,
        PropsPatch {
            visible: Some(false),
            ..rect(0.0, 0.0, 10.0, 10.0)
        },
    );
    let bytes = pdf(&mut Renderer::new(), &mut p, &[big]);
    assert_eq!(media_boxes(&bytes), [(MAX_PDF_SIDE, 144.0)]);
    let scene = p.scene();
    let mut r = Renderer::new();
    let run = |r: &mut Renderer, ids: &[NodeId], b: &ExportBudget| {
        r.export_pdf(&scene, ids, &NoImages, b)
    };
    assert_eq!(
        run(&mut r, &[hidden], &ExportBudget::unlimited()),
        Err(ExportError::Empty)
    );
    assert_eq!(
        run(&mut r, &[], &ExportBudget::unlimited()),
        Err(ExportError::NotFound)
    );
    /// Часы, которые всегда «после срока».
    struct Late(Cell<f64>);
    impl Clock for Late {
        fn now_ms(&self) -> f64 {
            self.0.set(self.0.get() + 1.0);
            self.0.get()
        }
    }
    let late = Late(Cell::new(10.0));
    let expired = ExportBudget {
        max_bytes: usize::MAX,
        clock: Some(&late),
        deadline_ms: 0.0,
    };
    assert_eq!(run(&mut r, &[big], &expired), Err(ExportError::Timeout));
    let small = ExportBudget {
        max_bytes: 100,
        ..ExportBudget::unlimited()
    };
    assert_eq!(run(&mut r, &[big], &small), Err(ExportError::TooBig));
}

/// Таблицы ToUnicode каждого шрифта: «<код> <UTF-16BE>» из bfchar.
fn to_unicode(bytes: &[u8]) -> Vec<HashMap<u16, String>> {
    let mut maps = Vec::new();
    for s in streams(bytes) {
        let t = text(&s);
        let Some(i) = t.find("beginbfchar") else {
            continue;
        };
        let mut map = HashMap::new();
        let body = &t[i..t.find("endbfchar").unwrap()];
        for line in body.lines().skip(1) {
            let hex: Vec<&str> = line
                .split(['<', '>'])
                .map(str::trim)
                .filter(|h| !h.is_empty())
                .collect();
            let [code, uni] = hex[..] else { continue };
            let units: Vec<u16> = (0..uni.len())
                .step_by(4)
                .map(|i| u16::from_str_radix(&uni[i..i + 4], 16).unwrap())
                .collect();
            map.insert(
                u16::from_str_radix(code, 16).unwrap(),
                String::from_utf16(&units).unwrap(),
            );
        }
        maps.push(map);
    }
    maps
}

/// Что покажет программа просмотра: коды глифов страниц (в порядке потока) через
/// ToUnicode; для глифа в `/ActualText` — подставленный им текст. Пары —
/// (текст, x начала показа в pt) в порядке потока.
fn shown(bytes: &[u8]) -> Vec<(String, f64)> {
    let all = streams(bytes);
    let map: HashMap<u16, String> = to_unicode(bytes).into_iter().flatten().collect();
    let mut out = Vec::new();
    for s in &all {
        let t = text(s);
        if !t.contains("BT") {
            continue;
        }
        let (mut actual, mut x): (Option<String>, f64) = (None, 0.0);
        let mut at = 0;
        while at < t.len() {
            let rest = &t[at..];
            if let Some(r) = rest.strip_prefix("/ActualText<FEFF") {
                let hex = &r[..r.find('>').unwrap()];
                let units: Vec<u16> = (0..hex.len())
                    .step_by(4)
                    .map(|i| u16::from_str_radix(&hex[i..i + 4], 16).unwrap())
                    .collect();
                actual = Some(String::from_utf16(&units).unwrap());
                at += 1;
            } else if rest.starts_with(" Tm") {
                // «… 1 0 0 -1 X Y Tm»: x — третье число с конца.
                let nums: Vec<&str> = t[..at].split_whitespace().rev().take(2).collect();
                x = nums[1].parse().unwrap_or(0.0);
                at += 1;
            } else if rest.starts_with('(') {
                // Строка PDF: двухбайтовые коды, экранирование `\ddd`, `\(`, `\)`.
                let (mut bytes, mut i) = (Vec::new(), 1);
                let b = rest.as_bytes();
                while b[i] != b')' {
                    if b[i] == b'\\' {
                        i += 1;
                        if b[i].is_ascii_digit() {
                            let mut v = 0u32;
                            let mut n = 0;
                            while n < 3 && b[i].is_ascii_digit() {
                                v = v * 8 + (b[i] - b'0') as u32;
                                i += 1;
                                n += 1;
                            }
                            bytes.push(v as u8);
                            continue;
                        }
                    }
                    bytes.push(b[i]);
                    i += 1;
                }
                let mut shown = String::new();
                match actual.take() {
                    Some(a) => shown = a,
                    None => {
                        for pair in bytes.chunks(2) {
                            let code = u16::from_be_bytes([pair[0], pair[1]]);
                            shown += map.get(&code).map_or("?", String::as_str);
                        }
                    }
                }
                out.push((shown, x));
                at += i + 1;
            } else {
                at += rest.chars().next().unwrap().len_utf8();
            }
        }
    }
    out
}

/// Лигатура Inter «->» (одна стрелка вместо двух символов) попадает в ToUnicode
/// обоими символами: копирование даёт «->», а не «-» или пустоту.
#[test]
fn a_ligature_keeps_all_its_characters_in_the_text_layer() {
    let mut p = Page::new();
    let id = add_text(
        &mut p,
        "a->b",
        "Regular",
        vec![Fill::solid(Color::BLACK)],
        0.0,
    );
    let mut r = Renderer::new();
    r.set_fonts(inter());
    let bytes = pdf(&mut r, &mut p, &[id]);
    let got: Vec<String> = shown(&bytes).into_iter().map(|(t, _)| t).collect();
    // Глифов три (a, стрелка, b), а символов четыре.
    assert_eq!(got.concat(), "a->b", "{got:?}");
    // Стрелка — одним глифом, у которого в ToUnicode оба символа (U+002D U+003E).
    assert!(
        streams(&bytes)
            .iter()
            .any(|c| text(c).contains("<002D003E>")),
        "ToUnicode стрелки"
    );
    assert!(!got.iter().any(|t| t.contains('?')), "{got:?}");
}

/// Справа налево: глифы идут в потоке слева направо (в визуальном порядке), а
/// символы каждого — свои; ничего не потеряно и не продублировано.
#[test]
fn right_to_left_runs_map_every_glyph_to_its_own_character() {
    let mut p = Page::new();
    // В Inter нет еврейских букв, в Open Sans — есть.
    let id = add_text_in(
        &mut p,
        "Open Sans",
        "שלום",
        "Regular",
        vec![Fill::solid(Color::BLACK)],
        0.0,
    );
    let mut r = Renderer::new();
    r.set_fonts(family(
        "Open Sans",
        &[
            "opensans/OpenSans-Variable.ttf",
            "opensans/OpenSans-Italic-Variable.ttf",
        ],
    ));
    let bytes = pdf(&mut r, &mut p, &[id]);
    let got = shown(&bytes);
    let xs: Vec<f64> = got.iter().map(|(_, x)| *x).collect();
    assert!(
        xs.windows(2).all(|w| w[0] <= w[1]),
        "поток слева направо: {xs:?}"
    );
    let visual: String = got.iter().map(|(t, _)| t.as_str()).collect();
    // Слева направо на странице: «ם», «ו», «ל», «ש» — обратный порядок строки.
    assert_eq!(visual, "םולש", "{got:?}");
    let reversed: String = "שלום".chars().rev().collect();
    assert_eq!(visual, reversed);
    assert!(
        streams(&bytes)
            .iter()
            .any(|s| text(s).contains("ReversedChars")),
        "обратный порядок помечен"
    );
}

/// Файл шрифта с PostScript-именем (name ID 6) из `name`: таблица
/// `name` заменяется одной записью и дописывается в конец файла.
fn with_postscript_name(mut font: Vec<u8>, name: &str) -> Vec<u8> {
    let be16 = |v: usize| (v as u16).to_be_bytes();
    let utf16 = |t: &str| -> Vec<u8> { t.encode_utf16().flat_map(u16::to_be_bytes).collect() };
    // Имена семейства и начертания остаются, чтобы шрифт загружался.
    let records = [
        (1, utf16("PT Sans")),
        (2, utf16("Regular")),
        (4, utf16("PT Sans")),
        (6, utf16(name)),
    ];
    let mut table = Vec::new();
    // format 0, число записей, начало строк; Windows, Unicode BMP, en-US.
    for v in [0, records.len(), 6 + 12 * records.len()] {
        table.extend(be16(v));
    }
    let mut at = 0;
    for (id, bytes) in &records {
        for v in [3, 1, 0x409, *id, bytes.len(), at] {
            table.extend(be16(v));
        }
        at += bytes.len();
    }
    for (_, bytes) in &records {
        table.extend(bytes);
    }
    while !font.len().is_multiple_of(4) {
        font.push(0);
    }
    let offset = font.len();
    let tables = u16::from_be_bytes([font[4], font[5]]) as usize;
    let at = (0..tables)
        .map(|i| 12 + 16 * i)
        .find(|r| &font[*r..*r + 4] == b"name")
        .expect("таблица name");
    font[at + 8..at + 12].copy_from_slice(&(offset as u32).to_be_bytes());
    font[at + 12..at + 16].copy_from_slice(&(table.len() as u32).to_be_bytes());
    font.extend(table);
    font
}

/// `krilla` режет PostScript-имя по байтам (на 109-м) и падает, если там
/// середина символа; шрифты приходят от пользователя, а паника в воркере
/// убивает ядро. Такой шрифт — текст контурами, без падения.
#[test]
fn a_font_with_an_unsafe_postscript_name_falls_back_to_outlines() {
    let file = "ptsans/PTSans-Regular.ttf";
    let run = |name: &str| {
        let mut p = Page::new();
        let id = add_text_in(
            &mut p,
            "PT Sans",
            "Привет",
            "Regular",
            vec![Fill::solid(Color::BLACK)],
            0.0,
        );
        let mut r = Renderer::new();
        r.set_fonts(family_with(
            "PT Sans",
            &[(file, font_file(file))],
            &|bytes| with_postscript_name(bytes, name),
        ));
        pdf(&mut r, &mut p, &[id])
    };
    // 60 символов «Ж» — 120 байт: 109-й байт внутри символа.
    let outlined = run(&"Ж".repeat(60));
    let raw = text(&outlined);
    assert!(!raw.contains("/FontFile"), "шрифт не встроен");
    assert!(to_unicode(&outlined).is_empty(), "текстового слоя нет");
    let content: Vec<String> = streams(&outlined).iter().map(|s| text(s)).collect();
    assert!(
        content.iter().any(|c| c.matches("\nf\n").count() >= 6),
        "буквы нарисованы контурами"
    );
    // Для сравнения: короткое имя — тот же шрифт встроен настоящим текстом.
    assert!(text(&run("PTSans-Regular")).contains("/FontFile"));
}

fn tiled_pixel(p: &mut Page, size: f64) -> (NodeId, HashMap<String, Arc<Pixmap>>) {
    let mut px = Pixmap::new(1, 1);
    px.data_as_u8_slice_mut().copy_from_slice(&[0, 128, 0, 255]);
    let png = px.into_png().unwrap();
    let hash = p.doc.images.add(png.clone());
    let id = p.add(
        None,
        NodeKind::Rect,
        PropsPatch {
            fills: Some(vec![Fill {
                scale_mode: ScaleMode::Tile,
                ..Fill::image(hash.clone())
            }]),
            ..rect(0.0, 0.0, size, size)
        },
    );
    (
        id,
        HashMap::from([(hash, Arc::new(decode_png(&png).unwrap()))]),
    )
}

/// Плиток больше 4096 — картинка не пропадает: одна плитка — узор PDF.
#[test]
fn a_fill_with_too_many_tiles_becomes_a_pattern() {
    let mut p = Page::new();
    let (id, images) = tiled_pixel(&mut p, 100.0);
    let scene = p.scene();
    let bytes = Renderer::new()
        .export_pdf(&scene, &[id], &images, &ExportBudget::unlimited())
        .unwrap();
    let raw = text(&bytes);
    assert!(raw.contains("/PatternType 1"), "узор");
    assert_eq!(raw.matches("/Subtype/Image").count(), 1);
    let content: Vec<String> = streams(&bytes).iter().map(|s| text(s)).collect();
    let draws: usize = content.iter().map(|c| c.matches(" Do").count()).sum();
    assert!(draws >= 1, "картинка нарисована");
    assert!(draws < 10, "а не 10 000 раз: {draws}");
}

/// Предел размера и часы действуют по ходу рисования, а не после него: много
/// плиток — ошибка раньше, чем страница дорисована.
#[test]
fn the_budget_stops_tiled_fills_while_drawing() {
    let mut p = Page::new();
    let (first, images) = tiled_pixel(&mut p, 60.0);
    let mut ids = vec![first];
    for _ in 0..400 {
        let (id, _) = tiled_pixel(&mut p, 60.0);
        ids.push(id);
    }
    let scene = p.scene();
    let mut r = Renderer::new();
    let small = ExportBudget {
        max_bytes: 200_000,
        ..ExportBudget::unlimited()
    };
    // 400 заливок по 3600 плиток — оценка перевалит предел на первых же.
    let started = std::time::Instant::now();
    assert_eq!(
        r.export_pdf(&scene, &ids, &images, &small),
        Err(ExportError::TooBig)
    );
    assert!(started.elapsed().as_secs_f64() < 5.0);
    /// Часы, что «истекают» после нескольких чтений.
    struct Soon(Cell<u32>);
    impl Clock for Soon {
        fn now_ms(&self) -> f64 {
            self.0.set(self.0.get() + 1);
            if self.0.get() > 3 {
                1e12
            } else {
                0.0
            }
        }
    }
    let clock = Soon(Cell::new(0));
    let timed = ExportBudget {
        max_bytes: usize::MAX,
        clock: Some(&clock),
        deadline_ms: 1e9,
    };
    assert_eq!(
        r.export_pdf(&scene, &ids, &images, &timed),
        Err(ExportError::Timeout)
    );
    // Часы читались мало: остановка по ходу, а не после всех 400 заливок.
    assert!(clock.0.get() < 200, "{}", clock.0.get());
}
