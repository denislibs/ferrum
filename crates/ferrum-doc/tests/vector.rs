//! Узел `Vector` и стиль обводки (подсистема 3, этап 3): двоичная форма сети с
//! проверкой не доверенных данных, запись в документ, зеркало против честной
//! перезагрузки из Loro и из кэша сцены, отмена, пределы, неверная сеть в файле.

use std::sync::Arc;

use ferrum_doc::container::{decode, encode};
use ferrum_doc::*;

/// Треугольник: три точки, три сегмента (один — кривая), область с петлёй.
fn triangle() -> VectorNetwork {
    VectorNetwork {
        points: vec![
            VectorPoint::new(0.0, 0.0),
            VectorPoint {
                handle_mode: HandleMode::MirrorAll,
                corner_radius: 2.5,
                ..VectorPoint::new(100.0, 0.0)
            },
            VectorPoint::new(50.0, 80.0),
        ],
        segments: vec![
            VectorSegment::line(0, 1),
            VectorSegment {
                start: 1,
                end: 2,
                tangent_start: [10.0, 20.0],
                tangent_end: [-5.0, 0.0],
            },
            VectorSegment::line(2, 0),
        ],
        regions: vec![VectorRegion {
            rule: FillRule::EvenOdd,
            loops: vec![vec![0, 1, 2]],
            fills: Some(vec![Fill::solid(Color::rgba(255, 0, 0, 255))]),
        }],
    }
}

fn doc_with_vector() -> (Document, PageId, NodeId) {
    let mut doc = Document::new();
    let page = doc.pages()[0].0;
    let id = doc
        .transact(page, vec![], |tx| {
            let id = tx.create_with_props(
                None,
                None,
                &Props {
                    w: 100.0,
                    h: 80.0,
                    stroke_style: StrokeStyle {
                        cap_start: StrokeCap::Round,
                        cap_end: StrokeCap::ArrowTriangle,
                        join: StrokeJoin::Bevel,
                        miter_limit: 8.0,
                    },
                    vector: Some(Arc::new(triangle())),
                    ..Props::defaults(NodeKind::Vector)
                },
            )?;
            Ok(vec![id])
        })
        .unwrap()[0];
    (doc, page, id)
}

#[test]
fn network_round_trips_through_binary_and_base64() {
    let net = triangle();
    assert_eq!(VectorNetwork::decode(&net.encode()), Ok(net.clone()));
    assert_eq!(
        VectorNetwork::from_base64(&net.to_base64()),
        Ok(net.clone())
    );
    assert_eq!(net.loop_points(&[0, 1, 2]), Some(vec![0, 1, 2]));
    // Петля в обратном порядке сегментов тоже замкнута.
    assert_eq!(net.loop_points(&[2, 1, 0]), Some(vec![0, 2, 1]));
    assert_eq!(net.degrees(), vec![2, 2, 2]);
}

#[test]
fn untrusted_network_is_rejected() {
    let bad = |f: &dyn Fn(&mut VectorNetwork)| {
        let mut n = triangle();
        f(&mut n);
        VectorNetwork::decode(&n.encode()).is_err()
    };
    assert!(bad(&|n| n.points[0].x = f64::NAN));
    assert!(bad(&|n| n.points[0].y = 1e12));
    assert!(bad(&|n| n.points[1].corner_radius = -1.0));
    assert!(bad(&|n| n.segments[0].end = 9));
    assert!(bad(&|n| n.segments[0].end = 0));
    assert!(bad(&|n| n.segments[1].tangent_end = [f64::INFINITY, 0.0]));
    // Петля не замкнута: сегментов 0 и 1 мало для цикла.
    assert!(bad(&|n| n.regions[0].loops = vec![vec![0, 1]]));
    assert!(bad(&|n| n.regions[0].loops = vec![vec![]]));
    assert!(bad(&|n| n.regions[0].fills = Some(vec![Fill::image("ab")])));
    // Обрезанная и удлинённая форма, чужая версия.
    let bytes = triangle().encode();
    assert!(VectorNetwork::decode(&bytes[..bytes.len() - 1]).is_err());
    let mut long = bytes.clone();
    long.push(0);
    assert!(VectorNetwork::decode(&long).is_err());
    let mut version = bytes;
    version[0] = 2;
    assert!(VectorNetwork::decode(&version).is_err());
    // Заявленное число точек больше остатка байтов — отказ без выделения памяти.
    let mut huge = vec![1u8];
    huge.extend_from_slice(&(MAX_VECTOR_POINTS as u32).to_le_bytes());
    assert!(VectorNetwork::decode(&huge).is_err());
    let mut over = vec![1u8];
    over.extend_from_slice(&(MAX_VECTOR_POINTS as u32 + 1).to_le_bytes());
    assert_eq!(
        VectorNetwork::decode(&over),
        Err(VectorError::TooManyPoints)
    );
}

#[test]
fn vector_and_stroke_style_survive_reload_from_loro_and_cache() {
    let (mut doc, page, id) = doc_with_vector();
    let before = doc.page(page).unwrap().props(id).unwrap();
    assert_eq!(before.kind, NodeKind::Vector);
    assert_eq!(before.vector.as_deref(), Some(&triangle()));
    // Без кэша — из Loro.
    let snapshot = doc.page(page).unwrap().export_snapshot().unwrap();
    let loro = PageDoc::from_snapshot(&snapshot).unwrap();
    assert_eq!(loro.props(id).unwrap(), before);
    // Файл целиком — из кэша сцены.
    let mut opened = decode(&encode(&doc).unwrap()).unwrap().document;
    let p = opened.page(page).unwrap();
    assert!(p.loaded_from_cache());
    assert_eq!(p.props(id).unwrap(), before);
    assert!(!opened.is_read_only());
}

#[test]
fn neutral_stroke_style_is_not_written_and_parses_back() {
    let mut doc = Document::new();
    let page = doc.pages()[0].0;
    let id = doc
        .transact(page, vec![], |tx| {
            Ok(vec![tx.create_node(None, None, NodeKind::Rect)?])
        })
        .unwrap()[0];
    let p = doc.page(page).unwrap();
    let snapshot = p.export_snapshot().unwrap();
    let props = PageDoc::from_snapshot(&snapshot)
        .unwrap()
        .props(id)
        .unwrap();
    assert_eq!(props.stroke_style, StrokeStyle::default());
    assert!(props.vector.is_none());
}

#[test]
fn vector_edit_is_one_undo_step_and_foreign_vector_is_ignored() {
    let (mut doc, page, id) = doc_with_vector();
    let mut moved = triangle();
    moved.points[2].y = 90.0;
    doc.transact(page, vec![id], |tx| {
        tx.set_props(
            id,
            &PropsPatch {
                vector: Some(Arc::new(moved.clone())),
                h: Some(90.0),
                stroke_join: Some(StrokeJoin::Round),
                ..Default::default()
            },
        )?;
        Ok(vec![id])
    })
    .unwrap();
    let p = doc.page(page).unwrap().props(id).unwrap();
    assert_eq!(p.vector.as_deref(), Some(&moved));
    assert_eq!(p.stroke_style.join, StrokeJoin::Round);
    doc.undo().unwrap();
    let p = doc.page(page).unwrap().props(id).unwrap();
    assert_eq!(p.vector.as_deref(), Some(&triangle()));
    assert_eq!(p.stroke_style.join, StrokeJoin::Bevel);
    // Сеть в патче прямоугольника пропускается, остальное пишется.
    let rect = doc
        .transact(page, vec![], |tx| {
            let r = tx.create_node(None, None, NodeKind::Rect)?;
            tx.set_props(
                r,
                &PropsPatch {
                    vector: Some(Arc::new(triangle())),
                    stroke_cap_end: Some(StrokeCap::Circle),
                    ..Default::default()
                },
            )?;
            Ok(vec![r])
        })
        .unwrap()[0];
    let r = doc.page(page).unwrap().props(rect).unwrap();
    assert!(r.vector.is_none());
    assert_eq!(r.stroke_style.cap_end, StrokeCap::Circle);
}

#[test]
fn over_limit_network_is_refused_without_a_step() {
    let (mut doc, page, id) = doc_with_vector();
    let big = VectorNetwork {
        points: (0..=MAX_VECTOR_POINTS)
            .map(|i| VectorPoint::new(i as f64, 0.0))
            .collect(),
        ..Default::default()
    };
    let err = doc.transact(page, vec![id], |tx| {
        tx.set_props(
            id,
            &PropsPatch {
                vector: Some(Arc::new(big.clone())),
                ..Default::default()
            },
        )?;
        Ok(vec![id])
    });
    assert!(matches!(err, Err(DocError::VectorTooLarge)));
    let created = doc.transact(page, vec![], |tx| {
        Ok(vec![tx.create_with_props(
            None,
            None,
            &Props {
                vector: Some(Arc::new(big.clone())),
                ..Props::defaults(NodeKind::Vector)
            },
        )?])
    });
    assert!(matches!(created, Err(DocError::VectorTooLarge)));
    // Остался только шаг создания исходного вектора.
    doc.undo().unwrap();
    assert!(!doc.can_undo());
}

#[test]
fn invalid_network_from_an_edit_is_refused_with_a_message_without_a_node_id() {
    // Правка редактора (например, «Сплющить» сверх 10 000 петель) — понятное
    // сообщение без «Узел —».
    let (mut doc, page, id) = doc_with_vector();
    let mut net = VectorNetwork {
        points: vec![VectorPoint::new(0.0, 0.0), VectorPoint::new(10.0, 0.0)],
        segments: vec![VectorSegment::line(0, 1), VectorSegment::line(1, 0)],
        regions: vec![],
    };
    net.regions = (0..=MAX_VECTOR_LOOPS)
        .map(|_| VectorRegion {
            rule: FillRule::NonZero,
            loops: vec![vec![0, 1]],
            fills: None,
        })
        .collect();
    let err = doc
        .transact(page, vec![id], |tx| {
            tx.set_props(
                id,
                &PropsPatch {
                    vector: Some(Arc::new(net.clone())),
                    ..Default::default()
                },
            )?;
            Ok(vec![id])
        })
        .unwrap_err();
    let text = err.to_string();
    assert!(matches!(err, DocError::VectorRejected(_)), "{err:?}");
    assert!(!text.contains('—') && !text.starts_with("Узел"), "{text}");
    assert_eq!(
        text,
        "Правка вектора не применена: в векторе слишком много сегментов или областей"
    );
}

#[test]
fn invalid_network_in_file_hides_the_node_and_opens_read_only() {
    let (mut doc, page, id) = doc_with_vector();
    let p = doc.page(page).unwrap();
    // Петля области ссылается на сегмент, которого нет.
    let mut broken = triangle();
    broken.regions[0].loops = vec![vec![0, 1, 7]];
    for bytes in [vec![1, 2, 3], broken.encode()] {
        p.force_vector_bytes_for_tests(id, &bytes);
        // Снапшот без кэша сцены — путь чтения из Loro (как файл чужой сборки).
        let reopened = PageDoc::from_snapshot(&p.export_snapshot().unwrap()).unwrap();
        assert!(
            reopened.has_unknown_nodes(),
            "документ откроется только для чтения"
        );
        assert!(reopened.walk().is_empty(), "узел с неверной сетью скрыт");
    }
}

#[test]
fn containers_and_text_cannot_be_converted_to_a_vector() {
    let mut doc = Document::new();
    let page = doc.pages()[0].0;
    let ids = doc
        .transact(page, vec![], |tx| {
            let frame = tx.create_node(None, None, NodeKind::Frame)?;
            let group = tx.create_node(None, None, NodeKind::Group)?;
            let text = tx.create_node(None, None, NodeKind::Text)?;
            Ok(vec![frame, group, text])
        })
        .unwrap();
    for id in ids {
        let err = doc.transact(page, vec![], |tx| {
            tx.convert_to_vector(id, Arc::new(triangle()))?;
            Ok(vec![])
        });
        match err {
            Err(DocError::NotConvertible(s)) => assert_eq!(s, id.to_string()),
            other => panic!("{other:?}"),
        }
        assert!(DocError::NotConvertible(id.to_string())
            .to_string()
            .contains("нельзя превратить в вектор"));
    }
}
