//! Сводка 1000 выделенных узлов на странице в 100 000: стоимость от выделения,
//! а не от документа. Только release.
#![cfg(all(not(target_arch = "wasm32"), not(debug_assertions)))]

use std::time::Instant;

use ferrum_doc::{fixture, Document, NodeId};
use ferrum_scene::Scene;
use ferrum_wasm::summary::summarize;

#[test]
fn thousand_selected_is_cheap() {
    let mut doc = Document::new();
    let p = doc.pages()[0].0;
    let mut ids: Vec<NodeId> = vec![];
    doc.transact(p, vec![], |tx| {
        ids = fixture::fill(tx, 100_000)?;
        Ok(vec![])
    })
    .unwrap();
    let scene = Scene::from_page(doc.page(p).unwrap());
    let sel = &ids[..1000];
    let t = Instant::now();
    let s = summarize(&scene, sel);
    let cold = t.elapsed();
    assert_eq!(s.count, 1000);
    println!("summarize(1000) = {cold:?}");
    assert!(cold.as_millis() < 5, "сводка 1000 узлов: {cold:?}");
}
