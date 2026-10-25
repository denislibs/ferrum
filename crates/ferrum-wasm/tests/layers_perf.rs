//! Стоимость дерева слоёв на странице в 100 000 узлов: окно — от окна и пути
//! раскрытия, а не от документа; поиск после первого запроса — от кэша имён и
//! прошлых совпадений. Время меряется только в release: в debug (14 с, пороги
//! бессмысленны) тест не запускается.
#![cfg(all(not(target_arch = "wasm32"), not(debug_assertions)))]

use std::collections::HashSet;
use std::time::{Duration, Instant};

use ferrum_doc::{fixture, Document, NodeId, PropsPatch};
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

fn timed(l: &mut Layers, scene: &Scene, q: &str) -> (Duration, u32) {
    let sel = HashSet::new();
    l.set_filter(q);
    let t = Instant::now();
    let total = l.total(scene);
    let d = t.elapsed();
    l.window(scene, &sel);
    (d, total)
}

#[test]
fn window_queries_reuse_cached_rows() {
    let (mut scene, ids) = page(100_000);
    let sel = HashSet::new();
    let mut l = Layers::new();
    l.set_expanded(ids[0], true);
    l.set_window(0, 40);
    let w = l.window(&scene, &sel).unwrap();
    assert_eq!(w.total, 1000 + 99);
    let t = Instant::now();
    for i in 0..1000u32 {
        l.set_window(i % 1000, 40);
        l.window(&scene, &sel);
    }
    let per = t.elapsed() / 1000;
    eprintln!("per window query: {per:?}");
    assert!(per.as_micros() < 500, "окно: {per:?}");
    // Кадр перетаскивания: превью геометрии не пересобирает строки.
    let patch = PropsPatch {
        x: Some(5.0),
        ..Default::default()
    };
    let t = Instant::now();
    for _ in 0..100 {
        scene.set_preview(&[(ids[0], patch.clone())]);
        l.window(&scene, &sel);
    }
    eprintln!("per preview frame: {:?}", t.elapsed() / 100);
}

#[test]
fn search_is_cached_and_narrows() {
    let (scene, _) = page(100_000);
    let mut l = Layers::new();
    l.set_window(0, 40);
    // Холодный первый запрос: полный обход с построением кэша имён.
    let (cold, _) = timed(&mut l, &scene, "слой 42");
    eprintln!("cold first query: {cold:?}");
    // Тёплый запрос, не продолжающий прошлый: полный обход по кэшу.
    let (warm, n) = timed(&mut l, &scene, "экран 99");
    eprintln!("warm full-pass query: {warm:?} ({n} rows)");
    assert!(warm.as_millis() < 5, "тёплый запрос: {warm:?}");
    // Набор по букве осмысленного запроса: каждый шаг сужает прошлые совпадения.
    let mut typed = String::new();
    for ch in "экран 99".chars() {
        typed.push(ch);
        let (d, n) = timed(&mut l, &scene, &typed);
        eprintln!("typed {typed:?}: {d:?} ({n} rows)");
        // Первая буква — полный просмотр кэша (запрос короче прошлого), дальше сужение.
        let limit = if typed.chars().count() == 1 { 6 } else { 3 };
        assert!(d.as_millis() < limit, "набор {typed:?}: {d:?}");
    }
    // Набор, где первые буквы совпадают со всеми 100 000 имён (худший случай:
    // столько же строк на выходе).
    let mut worst = Duration::ZERO;
    typed.clear();
    for ch in "слой ".chars() {
        typed.push(ch);
        let (d, n) = timed(&mut l, &scene, &typed);
        eprintln!("typed {typed:?}: {d:?} ({n} rows)");
        worst = worst.max(d);
    }
    assert!(worst.as_millis() < 8, "набор по букве: {worst:?}");
    // Предварительный кэш: первый запрос после prepare_search тёплый.
    let mut l2 = Layers::new();
    let t = Instant::now();
    l2.prepare_search(&scene);
    eprintln!("prepare_search: {:?}", t.elapsed());
    let (first, _) = timed(&mut l2, &scene, "слой 42");
    eprintln!("first query after prepare: {first:?}");
    assert!(first.as_millis() < 5, "после prepare: {first:?}");
}

#[test]
fn committed_edits_keep_name_cache_warm() {
    let mut doc = Document::new();
    let p = doc.pages()[0].0;
    let mut ids = vec![];
    doc.transact(p, vec![], |tx| {
        ids = fixture::fill(tx, 100_000)?;
        Ok(vec![])
    })
    .unwrap();
    doc.take_changes(p);
    doc.clear_history();
    let mut scene = Scene::from_page(doc.page(p).unwrap());
    let mut l = Layers::new();
    l.set_window(0, 40);
    let (cold, _) = timed(&mut l, &scene, "слой 42");
    eprintln!("cold first query: {cold:?}");
    let mut worst = Duration::ZERO;
    for i in 0..10 {
        // Правка при активном поиске: перенос и переименование.
        let (kid, to, name) = (ids[1 + i], ids[100], format!("Слой 42 п{i}"));
        doc.transact(p, vec![], |tx| {
            tx.move_node(kid, Some(to), 0)?;
            tx.set_props(
                kid,
                &PropsPatch {
                    name: Some(name),
                    ..Default::default()
                },
            )?;
            Ok(vec![])
        })
        .unwrap();
        let c = doc.take_changes(p);
        scene.apply(&c, doc.page(p).unwrap());
        l.note_changes(&c, &scene);
        let t = Instant::now();
        l.total(&scene);
        let d = t.elapsed();
        l.window(&scene, &HashSet::new());
        worst = worst.max(d);
    }
    eprintln!("query after committed edit (warm): worst {worst:?}");
    assert_eq!(
        l.total(&scene),
        2000 + 10,
        "переименованные попали в результат"
    );
    assert!(worst.as_millis() < 5, "после правки: {worst:?}");
    // Для сравнения: правка без note_changes роняет кэш.
    let kid = ids[50];
    doc.transact(p, vec![], |tx| {
        tx.set_props(
            kid,
            &PropsPatch {
                x: Some(3.0),
                ..Default::default()
            },
        )?;
        Ok(vec![])
    })
    .unwrap();
    let c = doc.take_changes(p);
    scene.apply(&c, doc.page(p).unwrap());
    let t = Instant::now();
    l.total(&scene);
    eprintln!("after edit without note_changes (cold): {:?}", t.elapsed());
}
