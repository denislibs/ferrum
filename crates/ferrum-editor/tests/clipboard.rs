//! Буфер обмена: свой формат, вставка с новыми идентификаторами, проверка данных.

mod common;

use common::*;
use ferrum_doc::{Fill, PropsPatch};
use ferrum_editor::clipboard::{ClipPayload, ClipboardError, MAGIC};
use ferrum_editor::*;
use kurbo::Point;

fn copy_of(h: &mut H) -> Vec<u8> {
    h.ed.take_events();
    h.key("KeyC", ctrl());
    h.ed.take_events()
        .into_iter()
        .find_map(|e| match e {
            EditorEvent::Clipboard(b) => Some(b),
            _ => None,
        })
        .expect("копия в событиях")
}

#[test]
fn copy_paste_round_trip_with_new_ids_and_images() {
    let mut h = H::new();
    let f = h.frame(None, 100.0, 100.0, 300.0, 200.0);
    let img = h.rect(Some(f), 10.0, 10.0, 50.0, 50.0);
    let bytes = b"image bytes".to_vec();
    let hash = h.ed.add_image(bytes.clone());
    h.set(
        img,
        PropsPatch {
            fills: Some(vec![Fill::image(hash.clone())]),
            rotation: Some(30.0),
            ..Default::default()
        },
    );
    h.ed.select(&[f]);
    let clip = copy_of(&mut h);
    assert_eq!(&clip[..8], MAGIC);
    let payload = ClipPayload::decode(&clip).unwrap();
    assert_eq!(payload.nodes.len(), 2);
    assert_eq!(payload.nodes[1].parent, Some(0));
    assert_eq!(payload.images, vec![(hash.clone(), bytes.clone())]);
    assert_eq!(ClipPayload::decode(&payload.encode()).unwrap(), payload);
    // Вставка: новые узлы на том же месте (оно видно), выделены, один шаг.
    let before = h.state();
    let steps = h.ed.step_count();
    h.ed.select(&[]);
    h.ed.paste(&clip).unwrap();
    let roots = h.children(None);
    assert_eq!(roots.len(), 2);
    let copy = roots[1];
    assert_ne!(copy, f);
    assert_eq!(h.sel(), vec![copy]);
    let p = h.props(copy);
    assert_eq!((p.x, p.y, p.w, p.h), (100.0, 100.0, 300.0, 200.0));
    let inner = h.children(Some(copy));
    assert_eq!(inner.len(), 1);
    let q = h.props(inner[0]);
    assert_eq!((q.x, q.y, q.rotation), (10.0, 10.0, 30.0));
    assert_eq!(q.fills[0].image_hash.as_deref(), Some(hash.as_str()));
    h.assert_one_step(steps, &before);
    // В другой документ копия приносит байты изображения с собой.
    let mut other = H::new();
    other.ed.paste(&clip).unwrap();
    assert_eq!(
        other.ed.document().images.get(&hash),
        Some(bytes.as_slice())
    );
}

#[test]
fn paste_goes_into_selected_frame_or_to_view_center() {
    let mut h = H::new();
    let r = h.rect(None, 600.0, 600.0, 40.0, 40.0);
    let f = h.frame(None, 0.0, 0.0, 300.0, 300.0);
    h.ed.select(&[r]);
    let bytes = copy_of(&mut h);
    // Выделен фрейм: копия внутрь; исходное место вне фрейма — в его центр.
    h.ed.select(&[f]);
    h.ed.paste(&bytes).unwrap();
    let c = h.sel()[0];
    assert_eq!(h.parent(c), Some(f));
    assert_eq!((h.props(c).x, h.props(c).y), (130.0, 130.0));
    // Исходное место не видно — в центр видимой части.
    let mut cam = ferrum_render::Camera::new(VIEW, 1.0);
    cam.set_center(Point::new(5000.0, 5000.0));
    h.ed.set_camera(cam);
    h.ed.select(&[]);
    h.ed.paste(&bytes).unwrap();
    let d = h.sel()[0];
    assert_eq!(h.parent(d), None);
    assert_eq!((h.props(d).x, h.props(d).y), (4980.0, 4980.0));
}

#[test]
fn cut_is_copy_plus_delete() {
    let mut h = H::new();
    let r = h.rect(None, 100.0, 100.0, 40.0, 40.0);
    h.ed.select(&[r]);
    h.ed.take_events();
    h.key("KeyX", ctrl());
    let events = h.ed.take_events();
    let bytes = events
        .iter()
        .find_map(|e| match e {
            EditorEvent::Clipboard(b) => Some(b.clone()),
            _ => None,
        })
        .unwrap();
    assert!(h.children(None).is_empty());
    h.ed.paste(&bytes).unwrap();
    assert_eq!(h.children(None).len(), 1);
    assert_ne!(h.children(None)[0], r);
}

#[test]
fn hostile_payloads_are_rejected_without_changes() {
    let mut h = H::new();
    let r = h.rect(None, 100.0, 100.0, 40.0, 40.0);
    h.ed.select(&[r]);
    let good = copy_of(&mut h);
    let before = h.state();
    let steps = h.ed.step_count();
    let json_of = |bytes: &[u8]| {
        let len = u32::from_le_bytes(bytes[12..16].try_into().unwrap()) as usize;
        String::from_utf8(bytes[16..16 + len].to_vec()).unwrap()
    };
    let with_json = |json: &str| {
        let mut out = MAGIC.to_vec();
        out.extend_from_slice(&1u32.to_le_bytes());
        out.extend_from_slice(&(json.len() as u32).to_le_bytes());
        out.extend_from_slice(json.as_bytes());
        out.extend_from_slice(&0u32.to_le_bytes());
        out
    };
    let json = json_of(&good);
    let mut newer = good.clone();
    newer[8..12].copy_from_slice(&7u32.to_le_bytes());
    let cases: Vec<(Vec<u8>, &str)> = vec![
        (b"garbage".to_vec(), "не ferrum"),
        (good[..good.len() - 1].to_vec(), "обрыв"),
        (newer, "новее"),
        ([good.clone(), vec![0]].concat(), "лишние байты"),
        (
            with_json(&json.replace("\"parent\":null", "\"parent\":5")),
            "ссылка вперёд",
        ),
        (with_json(&json.replace("#d9d9d9ff", "#zz")), "цвет"),
        (
            with_json("{\"nodes\":[],\"images\":[],\"bounds\":[0,0,1,1]}"),
            "пусто",
        ),
        (with_json("{\"nodes\":1}"), "не тот JSON"),
        (
            with_json(&json.replace("\"bounds\":[", "\"bounds\":[1e999,")),
            "границы",
        ),
    ];
    for (bytes, what) in cases {
        h.ed.take_events();
        assert!(h.ed.paste(&bytes).is_err(), "{what}");
        assert!(
            h.ed.take_events()
                .iter()
                .any(|e| matches!(e, EditorEvent::Error(_))),
            "{what}: ошибка в событиях"
        );
        assert_eq!(h.state(), before, "{what}");
        assert_eq!(h.ed.step_count(), steps, "{what}");
    }
    assert!(matches!(
        ClipPayload::decode(b"garbage"),
        Err(ClipboardError::NotFerrum)
    ));
    // Неизвестный тип пропускается с поддеревом; огромные числа приводятся к пределам.
    let unknown = json
        .replacen("\"kind\":\"rect\"", "\"kind\":\"star\"", 1)
        .replace("\"w\":40.0", "\"w\":1e300");
    assert!(matches!(
        ClipPayload::decode(&with_json(&unknown)),
        Err(ClipboardError::Empty)
    ));
    let huge = json.replace("\"w\":40.0", "\"w\":1e300");
    let p = ClipPayload::decode(&with_json(&huge)).unwrap();
    assert_eq!(p.nodes[0].props.w, ferrum_scene::shape::COORD_LIMIT);
}

#[test]
fn cut_is_one_undo_step_and_paste_gives_all_nodes_new_ids() {
    let mut h = H::new();
    let f = h.frame(None, 100.0, 100.0, 300.0, 200.0);
    let a = h.rect(Some(f), 10.0, 10.0, 50.0, 50.0);
    let b = h.rect(Some(f), 80.0, 10.0, 50.0, 50.0);
    h.ed.select(&[f]);
    let before = h.state();
    let steps = h.ed.step_count();
    h.ed.take_events();
    h.key("KeyX", ctrl());
    let bytes =
        h.ed.take_events()
            .into_iter()
            .find_map(|e| match e {
                EditorEvent::Clipboard(b) => Some(b),
                _ => None,
            })
            .unwrap();
    assert!(h.children(None).is_empty());
    h.assert_one_step(steps, &before);
    // Отмена возвращает исходные узлы, вставка рядом даёт полностью новые id.
    h.key("KeyZ", ctrl());
    assert_eq!(h.children(None), vec![f]);
    let steps = h.ed.step_count();
    h.ed.paste(&bytes).unwrap();
    assert_eq!(h.ed.step_count(), steps + 1);
    let copy = h.sel()[0];
    let inner = h.children(Some(copy));
    assert_eq!(inner.len(), 2);
    for id in [copy, inner[0], inner[1]] {
        assert!(![f, a, b].contains(&id), "новый идентификатор");
    }
    h.assert_scene_clean();
}

#[test]
fn failed_paste_keeps_images_and_history_untouched() {
    let mut h = H::new();
    let r = h.rect(None, 100.0, 100.0, 40.0, 40.0);
    h.ed.select(&[r]);
    let good = copy_of(&mut h);
    let steps = h.ed.step_count();
    let images = h.ed.document().images.len();
    let mut bad = good.clone();
    bad.truncate(good.len() - 3);
    assert!(h.ed.paste(&bad).is_err());
    assert_eq!(h.ed.step_count(), steps);
    assert_eq!(h.ed.document().images.len(), images);
    h.key("KeyZ", ctrl());
    assert_eq!(
        h.children(None),
        Vec::<ferrum_doc::NodeId>::new(),
        "отмена создания r, а не вставки"
    );
}

#[test]
fn copy_and_cut_with_missing_image_fail_without_changes() {
    let mut h = H::new();
    let r = h.rect(None, 100.0, 100.0, 40.0, 40.0);
    h.set(
        r,
        PropsPatch {
            fills: Some(vec![Fill::image("0".repeat(64))]),
            ..Default::default()
        },
    );
    h.ed.select(&[r]);
    let steps = h.ed.step_count();
    let before = h.state();
    for code in ["KeyC", "KeyX"] {
        h.ed.take_events();
        h.key(code, ctrl());
        let ev = h.ed.take_events();
        assert!(
            ev.iter().any(|e| matches!(e, EditorEvent::Error(_))),
            "{code}"
        );
        assert!(
            !ev.iter().any(|e| matches!(e, EditorEvent::Clipboard(_))),
            "{code}"
        );
        assert_eq!(h.children(None), vec![r], "{code}");
        assert_eq!(h.state(), before, "{code}");
        assert_eq!(h.ed.step_count(), steps, "{code}");
    }
}

#[test]
fn paste_skips_locked_and_hidden_containers() {
    let mut h = H::new();
    let r = h.rect(None, 600.0, 600.0, 40.0, 40.0);
    let outer = h.frame(None, 0.0, 0.0, 400.0, 400.0);
    let inner = h.frame(Some(outer), 10.0, 10.0, 200.0, 200.0);
    h.ed.select(&[r]);
    let bytes = copy_of(&mut h);
    // Заблокированный выделенный фрейм: вставка в ближайшего подходящего предка.
    h.set(
        inner,
        PropsPatch {
            locked: Some(true),
            ..Default::default()
        },
    );
    h.ed.select(&[inner]);
    h.ed.paste(&bytes).unwrap();
    let c = h.sel()[0];
    assert_eq!(h.parent(c), Some(outer));
    // Скрытый внешний и заблокированный внутренний: в корень.
    h.set(
        outer,
        PropsPatch {
            visible: Some(false),
            ..Default::default()
        },
    );
    h.ed.select(&[inner]);
    h.ed.paste(&bytes).unwrap();
    let d = h.sel()[0];
    assert_eq!(h.parent(d), None);
    // Выбранный не-фрейм внутри скрытого фрейма: тоже в корень.
    h.ed.select(&[c]);
    h.ed.paste(&bytes).unwrap();
    let e = h.sel()[0];
    assert_eq!(h.parent(e), None);
    // Скрытый выделенный фрейм сам по себе.
    let hid = h.frame(None, 1000.0, 0.0, 100.0, 100.0);
    h.set(
        hid,
        PropsPatch {
            visible: Some(false),
            ..Default::default()
        },
    );
    h.ed.select(&[hid]);
    h.ed.paste(&bytes).unwrap();
    let f = h.sel()[0];
    assert_eq!(h.parent(f), None);
}
