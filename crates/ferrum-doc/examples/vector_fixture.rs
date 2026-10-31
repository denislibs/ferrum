//! Файл `.ferrum` с векторным узлом — для проверки совместимости: сборка без
//! векторов (этап 2 подсистемы 3, `main` до этапа 3) открывает его только для
//! чтения (спека подсистемы 3, раздел 9). `cargo run -p ferrum-doc --example
//! vector_fixture -- <путь>`; проверку старой сборкой делает
//! `scripts/compat-old-build.sh <ref> vector_fixture`.

use std::sync::Arc;

use ferrum_doc::container::encode;
use ferrum_doc::{
    Document, NodeKind, Props, PropsPatch, StrokeCap, VectorNetwork, VectorPoint, VectorSegment,
};

fn main() {
    let path = std::env::args().nth(1).expect("путь к файлу");
    let mut doc = Document::new();
    let page = doc.pages()[0].0;
    doc.transact(page, vec![], |tx| {
        let rect = tx.create_node(None, None, NodeKind::Rect)?;
        tx.set_props(
            rect,
            &PropsPatch {
                name: Some("Прямоугольник".into()),
                ..Default::default()
            },
        )?;
        let mut arrow = Props {
            name: "Стрелка".into(),
            y: 150.0,
            w: 120.0,
            vector: Some(Arc::new(VectorNetwork {
                points: vec![VectorPoint::new(0.0, 0.0), VectorPoint::new(120.0, 0.0)],
                segments: vec![VectorSegment::line(0, 1)],
                regions: vec![],
            })),
            ..Props::defaults(NodeKind::Vector)
        };
        arrow.stroke_style.cap_end = StrokeCap::ArrowLines;
        tx.create_with_props(None, None, &arrow)?;
        Ok(vec![])
    })
    .unwrap();
    std::fs::write(&path, encode(&doc).unwrap()).unwrap();
    println!("{path}: прямоугольник и вектор");
}
