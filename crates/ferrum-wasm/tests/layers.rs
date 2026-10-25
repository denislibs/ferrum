//! Дерево слоёв в worker'е: окно строк ограничено, свёрнутый фрейм — одна строка,
//! раскрытие, поиск с предками, диапазон для Shift, раскрытие предков выделения
//! с холста; одинаковое окно второй раз не отправляется.
#![cfg(not(target_arch = "wasm32"))]

use std::collections::HashSet;

use ferrum_doc::{fixture, Document, NodeId};
use ferrum_scene::Scene;
use ferrum_wasm::layers::{Layers, NodeType, MAX_LAYER_WINDOW};

/// Страница-пример: фреймы «Экран N» по 99 фигур «Слой i».
fn page(nodes: usize) -> (Scene, Vec<NodeId>) {
    let mut doc = Document::new();
    let p = doc.pages()[0].0;
    let mut ids = vec![];
    doc.transact(p, vec![], |tx| {
        ids = fixture::fill(tx, nodes)?;
        Ok(vec![])
    })
    .unwrap();
    (Scene::from_page(doc.page(p).unwrap()), ids)
}

fn none() -> HashSet<NodeId> {
    HashSet::new()
}

#[test]
fn collapsed_frames_are_one_row_each_top_first() {
    let (scene, _) = page(1000);
    let mut l = Layers::new();
    l.set_window(0, 5);
    let w = l.window(&scene, &none()).unwrap();
    assert_eq!(w.total, 10, "10 фреймов, дети свёрнуты");
    assert_eq!(w.rows.len(), 5);
    assert_eq!(w.rows[0].name, "Экран 9", "верхний — первым");
    assert_eq!((w.rows[0].kind, w.rows[0].depth), (NodeType::Frame, 0));
    assert!(w.rows[0].has_children && !w.rows[0].expanded);
    assert!(
        l.window(&scene, &none()).is_none(),
        "то же окно не повторяется"
    );
    l.resend();
    assert!(l.window(&scene, &none()).is_some());
}

#[test]
fn window_is_capped_and_clamped() {
    let (scene, ids) = page(1000);
    let mut l = Layers::new();
    l.set_expanded(ids[0], true);
    l.set_window(0, 10_000);
    let w = l.window(&scene, &none()).unwrap();
    assert_eq!(w.total, 10 + 99);
    assert!(w.rows.len() as u32 <= MAX_LAYER_WINDOW);
    l.set_window(5000, 20);
    let w = l.window(&scene, &none()).unwrap();
    assert_eq!(
        (w.start, w.rows.len()),
        (109, 0),
        "начало за концом — пустое окно"
    );
}

#[test]
fn expanded_frame_lists_children_top_first() {
    let (scene, ids) = page(200);
    let mut l = Layers::new();
    // ids[0] — «Экран 0», нижний фрейм; его строка — вторая.
    l.set_expanded(ids[0], true);
    l.set_window(0, 4);
    let w = l.window(&scene, &none()).unwrap();
    let names: Vec<&str> = w.rows.iter().map(|r| r.name.as_str()).collect();
    assert_eq!(names, ["Экран 1", "Экран 0", "Слой 98", "Слой 97"]);
    assert_eq!(w.rows[2].depth, 1);
    let mut sel = HashSet::new();
    sel.insert(ids[0]);
    let w = l.window(&scene, &sel).unwrap();
    assert!(w.rows[1].selected && w.rows[2].parent_selected && !w.rows[2].selected);
}

#[test]
fn search_shows_matches_with_ancestors() {
    let (scene, _) = page(300);
    let mut l = Layers::new();
    l.set_filter("  слой 42 ");
    l.set_window(0, 50);
    let w = l.window(&scene, &none()).unwrap();
    // В каждом из трёх фреймов «Слой 42» и его фрейм.
    assert_eq!(w.total, 6);
    assert_eq!(w.rows[0].name, "Экран 2");
    assert!(w.rows[0].expanded, "при поиске всё раскрыто");
    assert_eq!((w.rows[1].name.as_str(), w.rows[1].depth), ("Слой 42", 1));
    l.set_filter("");
    assert_eq!(l.window(&scene, &none()).unwrap().total, 3);
}

#[test]
fn range_and_reveal() {
    let (scene, ids) = page(200);
    let mut l = Layers::new();
    // Выделение с холста внутри «Экран 0»: фрейм раскрывается, строка — 2-я снизу
    // вверх по фрейму.
    let child = ids[5];
    let at = l.reveal(&scene, &[child]).unwrap();
    l.set_window(at, 1);
    let w = l.window(&scene, &none()).unwrap();
    assert_eq!(w.rows[0].id, child.to_string());
    // От «Экран 1» (первая строка) до «Слой 0» (нижний ребёнок «Экран 0»).
    let rows = l.range(&scene, ids[100], ids[1]);
    assert_eq!(rows.len(), 1 + 1 + 99, "оба фрейма и дети раскрытого");
    assert_eq!((rows[0], rows[100]), (ids[100], ids[1]));
    assert_eq!(
        l.range(&scene, ids[1], ids[100]),
        rows,
        "в любом направлении"
    );
    let missing = ids[150];
    assert_eq!(
        l.range(&scene, missing, ids[0]),
        vec![ids[0]],
        "свёрнутый — только цель"
    );
}

#[test]
fn rows_follow_frames_not_nodes() {
    let (scene, ids) = page(30_000);
    let mut l = Layers::new();
    l.set_expanded(ids[0], true);
    l.set_window(0, 40);
    let w = l.window(&scene, &none()).unwrap();
    assert_eq!(w.total, 300 + 99);
    assert_eq!(w.rows.len(), 40);
    // Строк — по числу фреймов и раскрытых детей, не по числу узлов.
    assert_eq!(w.rows[1].name, "Экран 298");
}
