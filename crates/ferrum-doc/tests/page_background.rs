//! Фон страницы (этап 5): цвет в метаданных страницы, без записи нейтрального
//! значения, переживает сохранение; последнюю страницу удалить нельзя. И правка
//! свойств, которая ничего не меняет, не создаёт шага отмены (поля панелей
//! применяют значение и при уходе фокуса).

use ferrum_doc::*;

#[test]
fn background_is_unset_by_default_and_round_trips() {
    let mut doc = Document::new();
    let page = doc.pages()[0].0;
    assert_eq!(doc.page_background(page), None, "по умолчанию — цвет темы");
    let c = Color::parse("#ff8800").unwrap();
    doc.set_page_background(page, Some(c)).unwrap();
    assert_eq!(doc.page_background(page), Some(c));
    let bytes = container::encode(&doc).unwrap();
    let opened = container::decode(&bytes).unwrap();
    assert_eq!(
        opened.document.page_background(page),
        Some(c),
        "фон сохраняется в файле"
    );
    doc.set_page_background(page, None).unwrap();
    assert_eq!(doc.page_background(page), None, "сброс снимает ключ");
}

#[test]
fn background_is_not_in_undo_history() {
    let mut doc = Document::new();
    let page = doc.pages()[0].0;
    doc.set_page_background(page, Some(Color::WHITE)).unwrap();
    assert!(
        !doc.can_undo(),
        "операции со страницами не попадают в историю"
    );
}

#[test]
fn unknown_page_and_last_page_are_errors() {
    let mut doc = Document::new();
    let page = doc.pages()[0].0;
    let other = doc.add_page("Вторая").unwrap();
    doc.delete_page(other).unwrap();
    assert!(matches!(
        doc.set_page_background(other, Some(Color::WHITE)),
        Err(DocError::PageNotFound(_))
    ));
    assert_eq!(doc.page_background(other), None);
    assert!(matches!(doc.delete_page(page), Err(DocError::LastPage)));
    assert_eq!(doc.pages().len(), 1);
}

#[test]
fn unchanged_props_are_not_a_step() {
    let mut doc = Document::new();
    let page = doc.pages()[0].0;
    let id = doc
        .transact(page, vec![], |tx| {
            Ok(vec![tx.create_node(None, None, NodeKind::Rect)?])
        })
        .unwrap()[0];
    doc.clear_history();
    let same = PropsPatch {
        opacity: Some(1.0),
        x: Some(0.0),
        ..Default::default()
    };
    doc.transact(page, vec![], |tx| {
        tx.set_props(id, &same)?;
        Ok(vec![])
    })
    .unwrap();
    assert!(!doc.can_undo(), "те же значения — не шаг");
    doc.transact(page, vec![], |tx| {
        tx.set_props(
            id,
            &PropsPatch {
                opacity: Some(0.5),
                ..same.clone()
            },
        )?;
        Ok(vec![])
    })
    .unwrap();
    assert!(doc.can_undo());
    doc.undo().unwrap();
    assert_eq!(doc.page(page).unwrap().props(id).unwrap().opacity, 1.0);
}
