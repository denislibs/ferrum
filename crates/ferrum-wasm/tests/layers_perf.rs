//! Стоимость дерева слоёв на странице в 100 000 узлов: окно — от окна и пути
//! раскрытия, а не от документа; поиск по запросу не блокирует worker надолго.
#![cfg(not(target_arch = "wasm32"))]

use std::collections::HashSet;
use std::time::Instant;

use ferrum_doc::{fixture, Document, NodeId};
use ferrum_scene::Scene;
use ferrum_wasm::layers::Layers;

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

#[test]
fn window_queries_reuse_cached_rows_and_search_is_bounded() {
    let (scene, ids) = page(100_000);
    let sel = HashSet::new();
    let mut l = Layers::new();
    l.set_expanded(ids[0], true);
    l.set_window(0, 40);
    let t = Instant::now();
    let w = l.window(&scene, &sel).unwrap();
    let build = t.elapsed();
    assert_eq!(w.total, 1000 + 99, "1000 фреймов и дети одного раскрытого");
    // Прокрутка: список строк не пересобирается, окно стоит O(окно).
    let t = Instant::now();
    for i in 0..1000u32 {
        l.set_window(i % 1000, 40);
        l.window(&scene, &sel);
    }
    let per_scroll = t.elapsed() / 1000;
    eprintln!("build rows: {build:?}; per window query: {per_scroll:?}");
    // Время меряется только в release: в debug пороги не имеют смысла.
    assert!(
        cfg!(debug_assertions) || per_scroll.as_micros() < 2000,
        "окно: {per_scroll:?}"
    );
    // Поиск по 100 000 именам.
    let mut worst = std::time::Duration::ZERO;
    for q in ["слой 42", "экран 999", "нет такого", "с"] {
        l.set_filter(q);
        let t = Instant::now();
        let w = l.window(&scene, &sel);
        let d = t.elapsed();
        eprintln!("search {q:?}: {d:?}, total={:?}", w.map(|w| w.total));
        worst = worst.max(d);
    }
    assert!(
        cfg!(debug_assertions) || worst.as_millis() < 50,
        "поиск: {worst:?}"
    );
}
