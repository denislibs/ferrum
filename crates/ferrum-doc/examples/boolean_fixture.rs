//! Файл `.ferrum` с булевой группой — для проверки совместимости: сборка без
//! булевых групп (`main` до этапа 4) открывает его только для чтения — группа с
//! детьми скрыта, как узел неизвестного типа (спека подсистемы 3, раздел 10).
//! `cargo run -p ferrum-doc --example boolean_fixture -- <путь>`; проверку старой
//! сборкой делает `scripts/compat-old-build.sh <ref> boolean_fixture`.

use ferrum_doc::container::encode;
use ferrum_doc::{BooleanOp, Document, NodeKind, Props, PropsPatch};

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
        let b = tx.create_with_props(
            None,
            None,
            &Props {
                name: BooleanOp::Subtract.name().into(),
                y: 150.0,
                boolean_op: Some(BooleanOp::Subtract),
                ..Props::defaults(NodeKind::Boolean)
            },
        )?;
        for (kind, x) in [(NodeKind::Rect, 0.0), (NodeKind::Ellipse, 50.0)] {
            let c = tx.create_node(Some(b), None, kind)?;
            tx.set_props(
                c,
                &PropsPatch {
                    x: Some(x),
                    ..Default::default()
                },
            )?;
        }
        Ok(vec![])
    })
    .unwrap();
    std::fs::write(&path, encode(&doc).unwrap()).unwrap();
    println!("{path}: прямоугольник и булева группа с двумя детьми");
}
