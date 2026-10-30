//! Файл `.ferrum` с текстовым узлом — для проверки совместимости: сборка без
//! текста (подсистема 1) открывает его только для чтения (спека подсистемы 3,
//! раздел 3). `cargo run -p ferrum-doc --example text_fixture -- <путь>`; проверку
//! старой сборкой делает `scripts/compat-old-build.sh`.

use ferrum_doc::container::encode;
use ferrum_doc::{Document, NodeKind, PropsPatch, TextAttrs, TextContent};

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
        let text = tx.create_node(None, None, NodeKind::Text)?;
        tx.set_text_content(text, &TextContent::plain("Текст из новой сборки"))?;
        tx.format_text(
            text,
            0..5,
            &TextAttrs {
                style: Some("Bold".into()),
                ..Default::default()
            },
            &[],
        )?;
        Ok(vec![])
    })
    .unwrap();
    std::fs::write(&path, encode(&doc).unwrap()).unwrap();
    println!("{path}: прямоугольник и текст");
}
