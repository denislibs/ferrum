//! Узел `Boolean` (подсистема 3, этап 4): операция в документе, зеркало против
//! перезагрузки из Loro и из кэша сцены, отмена, чужая операция в патче, не
//! доверенная операция в файле.

use ferrum_doc::container::{decode, encode};
use ferrum_doc::*;

/// Булева группа «Вычитание» с двумя прямоугольниками.
fn doc_with_boolean() -> (Document, PageId, NodeId) {
    let mut doc = Document::new();
    let page = doc.pages()[0].0;
    let id = doc
        .transact(page, vec![], |tx| {
            let b = tx.create_with_props(
                None,
                None,
                &Props {
                    name: "Вычитание".into(),
                    boolean_op: Some(BooleanOp::Subtract),
                    ..Props::defaults(NodeKind::Boolean)
                },
            )?;
            for x in [0.0, 50.0] {
                let r = tx.create_node(Some(b), None, NodeKind::Rect)?;
                tx.set_props(
                    r,
                    &PropsPatch {
                        x: Some(x),
                        ..Default::default()
                    },
                )?;
            }
            Ok(vec![b])
        })
        .unwrap()[0];
    (doc, page, id)
}

#[test]
fn op_strings_round_trip_and_unknown_is_rejected() {
    for op in BooleanOp::ALL {
        assert_eq!(BooleanOp::parse(op.as_str()), Some(op));
    }
    assert_eq!(BooleanOp::parse("divide"), None);
    assert_eq!(BooleanOp::parse("Union"), None);
    assert_eq!(NodeKind::parse("boolean"), Some(NodeKind::Boolean));
    assert!(NodeKind::Boolean.is_group_like() && NodeKind::Group.is_group_like());
    assert!(!NodeKind::Frame.is_group_like());
    let d = Props::defaults(NodeKind::Boolean);
    assert_eq!((d.w, d.h, d.boolean_op), (0.0, 0.0, Some(BooleanOp::Union)));
    assert_eq!(d.name, "Объединение");
    assert_eq!(Props::defaults(NodeKind::Group).boolean_op, None);
}

#[test]
fn boolean_survives_reload_from_loro_and_cache() {
    let (mut doc, page, id) = doc_with_boolean();
    let before = doc.page(page).unwrap().props(id).unwrap();
    assert_eq!(before.boolean_op, Some(BooleanOp::Subtract));
    assert_eq!(doc.page(page).unwrap().children(Some(id)).len(), 2);
    let snapshot = doc.page(page).unwrap().export_snapshot().unwrap();
    let loro = PageDoc::from_snapshot(&snapshot).unwrap();
    assert_eq!(loro.props(id).unwrap(), before);
    assert_eq!(loro.children(Some(id)).len(), 2);
    let mut opened = decode(&encode(&doc).unwrap()).unwrap().document;
    let p = opened.page(page).unwrap();
    assert!(p.loaded_from_cache());
    assert_eq!(p.props(id).unwrap(), before);
    assert!(!opened.is_read_only());
}

#[test]
fn op_edit_is_one_undo_step_and_foreign_op_is_ignored() {
    let (mut doc, page, id) = doc_with_boolean();
    doc.transact(page, vec![id], |tx| {
        tx.set_props(
            id,
            &PropsPatch {
                boolean_op: Some(BooleanOp::Exclude),
                name: Some("Исключение".into()),
                ..Default::default()
            },
        )?;
        Ok(vec![id])
    })
    .unwrap();
    let p = doc.page(page).unwrap().props(id).unwrap();
    assert_eq!(p.boolean_op, Some(BooleanOp::Exclude));
    doc.undo().unwrap();
    let p = doc.page(page).unwrap().props(id).unwrap();
    assert_eq!(p.boolean_op, Some(BooleanOp::Subtract));
    assert_eq!(p.name, "Вычитание");
    // Операция в патче прямоугольника пропускается, остальное пишется.
    let rect = doc.page(page).unwrap().children(Some(id))[0];
    doc.transact(page, vec![], |tx| {
        tx.set_props(
            rect,
            &PropsPatch {
                boolean_op: Some(BooleanOp::Intersect),
                x: Some(7.0),
                ..Default::default()
            },
        )?;
        Ok(vec![])
    })
    .unwrap();
    let r = doc.page(page).unwrap().props(rect).unwrap();
    assert_eq!((r.boolean_op, r.x), (None, 7.0));
    let reloaded =
        PageDoc::from_snapshot(&doc.page(page).unwrap().export_snapshot().unwrap()).unwrap();
    assert_eq!(reloaded.props(rect).unwrap().boolean_op, None);
}

#[test]
fn unknown_op_in_file_hides_the_subtree_and_opens_read_only() {
    let (mut doc, page, id) = doc_with_boolean();
    let p = doc.page(page).unwrap();
    for raw in ["divide", ""] {
        p.force_boolean_op_for_tests(id, raw);
        let reopened = PageDoc::from_snapshot(&p.export_snapshot().unwrap()).unwrap();
        assert!(reopened.has_unknown_nodes(), "только для чтения");
        assert!(reopened.walk().is_empty(), "группа с детьми скрыта");
    }
    // Известная операция снова читается.
    p.force_boolean_op_for_tests(id, "intersect");
    let reopened = PageDoc::from_snapshot(&p.export_snapshot().unwrap()).unwrap();
    assert!(!reopened.has_unknown_nodes());
    assert_eq!(
        reopened.props(id).unwrap().boolean_op,
        Some(BooleanOp::Intersect)
    );
    assert!(DocError::InvalidBooleanOp("1@2".into(), "divide".into())
        .to_string()
        .contains("«divide»"));
}

#[test]
fn boolean_cannot_be_converted_to_a_vector() {
    let (mut doc, page, id) = doc_with_boolean();
    let err = doc.transact(page, vec![], |tx| {
        tx.convert_to_vector(id, std::sync::Arc::new(VectorNetwork::default()))?;
        Ok(vec![])
    });
    assert!(matches!(err, Err(DocError::NotConvertible(_))), "{err:?}");
}
