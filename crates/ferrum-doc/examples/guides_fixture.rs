//! Файл `.ferrum` с направляющими у страницы и фрейма — для проверки совместимости:
//! сборка без направляющих открывает его как обычно, для записи, неизвестный ключ
//! просто не читается (спека подсистемы 2, раздел 9). Проверку старой сборкой делает
//! `scripts/compat-old-build.sh <ref> guides_fixture`.
//! `cargo run -p ferrum-doc --example guides_fixture -- <путь>` — записать файл;
//! `… -- --check <путь>` — файл, пересохранённый старой сборкой, хранит направляющие.

use ferrum_doc::container::{decode, encode};
use ferrum_doc::{Document, Guide, GuideAxis, GuideOwner, NodeId, NodeKind};

fn expected_page() -> Vec<Guide> {
    vec![
        Guide::new(GuideAxis::X, 100.0),
        Guide::new(GuideAxis::Y, -40.5),
    ]
}

fn expected_frame() -> Vec<Guide> {
    vec![Guide::new(GuideAxis::Y, 25.0)]
}

fn check(path: &str) {
    let bytes = std::fs::read(path).expect("файл");
    let mut doc = decode(&bytes).expect("файл читается").document;
    let page = doc.pages()[0].0;
    assert_eq!(
        doc.page_guides(page),
        expected_page(),
        "направляющие страницы"
    );
    let p = doc.page(page).expect("страница");
    let frame: NodeId = p
        .walk()
        .into_iter()
        .find(|id| p.kind(*id).ok() == Some(NodeKind::Frame))
        .expect("фрейм");
    let props = p.props(frame).expect("свойства");
    assert_eq!(
        props.guides.as_deref(),
        Some(&expected_frame()),
        "направляющие фрейма"
    );
    assert_eq!(p.walk().len(), 3, "старая сборка дописала эллипс");
    println!("{path}: направляющие на месте");
}

fn main() {
    let mut args = std::env::args().skip(1);
    let first = args.next().expect("путь к файлу");
    if first == "--check" {
        check(&args.next().expect("путь к файлу"));
        return;
    }
    let mut doc = Document::new();
    let page = doc.pages()[0].0;
    let frame = doc
        .transact(page, vec![], |tx| {
            tx.create_node(None, None, NodeKind::Rect)?;
            Ok(vec![tx.create_node(None, None, NodeKind::Frame)?])
        })
        .unwrap()[0];
    doc.set_guides(
        page,
        vec![],
        &[
            (GuideOwner::Page(page), expected_page()),
            (GuideOwner::Frame(frame), expected_frame()),
        ],
    )
    .unwrap();
    std::fs::write(&first, encode(&doc).unwrap()).unwrap();
    println!("{first}: прямоугольник, фрейм и направляющие");
}
