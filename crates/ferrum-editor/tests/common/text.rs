//! Текст в сценарных тестах (подсистема 3, этап 2): реестр с вшитыми Inter и
//! Roboto, текст в точке мира, раскладка правимого узла, состояние правки.
#![allow(dead_code)]

use std::path::PathBuf;

use ferrum_doc::{NodeId, TextContent, TextProps};
use ferrum_editor::TextEditInfo;
use ferrum_text::{group, read_faces, Fonts, LayoutRequest, SharedFonts, SourceKind, TextLayout};
use kurbo::Point;

use super::H;

pub fn font_file(rel: &str) -> Vec<u8> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../apps/web/public/fonts")
        .join(rel);
    std::fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

const FILES: [(&str, &str); 4] = [
    ("Inter", "inter/Inter-Variable.ttf"),
    ("Inter", "inter/Inter-Italic-Variable.ttf"),
    ("Roboto", "roboto/Roboto-Variable.ttf"),
    ("Roboto", "roboto/Roboto-Italic-Variable.ttf"),
];

/// Реестр со списком вшитых Inter и Roboto, оба загружены.
pub fn loaded_fonts() -> SharedFonts {
    let shared = Fonts::shared();
    {
        let mut f = shared.borrow_mut();
        let faces = FILES
            .iter()
            .flat_map(|(_, r)| read_faces(&font_file(r), r))
            .collect::<Vec<_>>();
        f.add_catalog(SourceKind::Bundled, group(faces));
        for (family, rel) in FILES {
            f.load(family, rel, font_file(rel)).unwrap();
        }
        f.take_requests();
    }
    shared
}

/// Стенд с загруженными шрифтами (экран совпадает с миром: зум 1).
pub fn text_h() -> H {
    let mut h = H::new();
    h.ed.set_fonts(loaded_fonts());
    h
}

/// Текст «авто по ширине» кеглем `size` в точке мира; выделен.
pub fn add_text(h: &mut H, x: f64, y: f64, s: &str, size: f64) -> NodeId {
    let mut props = TextProps::default();
    props.style.size = size;
    h.ed.create_text(Point::new(x, y), &TextContent::plain(s), &props, 0.0, 0.0)
        .unwrap()
}

/// Раскладка узла по свойствам сцены.
pub fn layout_of(h: &H, id: NodeId) -> TextLayout {
    let n = h.ed.scene().node(id).unwrap();
    let t = n.props.text.as_ref().unwrap();
    h.ed.fonts().borrow_mut().layout(&LayoutRequest::for_node(
        t,
        &n.props.fills,
        n.props.w,
        n.props.h,
    ))
}

/// Точка мира позиции `index` на первой строке её текста (середина строки).
pub fn at_index(h: &H, id: NodeId, index: usize) -> (f64, f64) {
    let l = layout_of(h, id);
    let line = l
        .carets
        .iter()
        .find(|c| c.start <= index && index <= c.end)
        .unwrap();
    let n = h.ed.scene().node(id).unwrap();
    let p = n.world * Point::new(line.x_of(index), (line.top + line.bottom) / 2.0);
    (p.x, p.y)
}

pub fn info(h: &H) -> TextEditInfo {
    h.ed.text_edit_info().expect("идёт правка текста")
}

/// Выделение правки: (якорь, курсор).
pub fn sel_range(h: &H) -> (usize, usize) {
    let i = info(h);
    (i.anchor, i.focus)
}

/// Текст узла в документе (с последним переводом строки).
pub fn text_of(h: &mut H, id: NodeId) -> String {
    h.ed.page_doc()
        .props(id)
        .unwrap()
        .text
        .unwrap()
        .content
        .text()
}
