//! Формат буфера обмена Ferrum: кодирование и разбор не доверенных данных.

use ferrum_doc::{Color, Fill, NodeKind, Props, Stroke, StrokeAlign};
use ferrum_editor::clipboard::*;
use kurbo::Rect;

fn sample() -> ClipPayload {
    let mut frame = Props::defaults(NodeKind::Frame);
    frame.x = 10.0;
    frame.rotation = 15.0;
    let mut pic = Props::defaults(NodeKind::Image);
    pic.fills = vec![Fill::image("ab".repeat(32))];
    pic.strokes = vec![Stroke {
        color: Color::rgba(1, 2, 3, 4),
        width: 2.0,
        align: StrokeAlign::Outside,
    }];
    ClipPayload {
        nodes: vec![
            ClipNode {
                parent: None,
                props: frame,
            },
            ClipNode {
                parent: Some(0),
                props: pic,
            },
            ClipNode {
                parent: None,
                props: Props::defaults(NodeKind::Line),
            },
        ],
        images: vec![("ab".repeat(32), vec![1, 2, 3])],
        bounds: Rect::new(10.0, 0.0, 110.0, 100.0),
    }
}

/// Байты с заменённым JSON (без изображений).
fn with_json(json: &str) -> Vec<u8> {
    let mut out = MAGIC.to_vec();
    out.extend_from_slice(&VERSION.to_le_bytes());
    out.extend_from_slice(&(json.len() as u32).to_le_bytes());
    out.extend_from_slice(json.as_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    out
}

fn json_of(bytes: &[u8]) -> String {
    let len = u32::from_le_bytes(bytes[12..16].try_into().unwrap()) as usize;
    String::from_utf8(bytes[16..16 + len].to_vec()).unwrap()
}

#[test]
fn encode_decode_round_trip() {
    let p = sample();
    let bytes = p.encode();
    assert_eq!(&bytes[..8], MAGIC);
    assert_eq!(ClipPayload::decode(&bytes).unwrap(), p);
}

#[test]
fn structural_damage_is_rejected() {
    let good = sample().encode();
    let mut newer = good.clone();
    newer[8..12].copy_from_slice(&(VERSION + 1).to_le_bytes());
    let mut count = good.clone();
    let at = 16 + json_of(&good).len();
    count[at..at + 4].copy_from_slice(&9u32.to_le_bytes());
    let mut huge_len = good.clone();
    huge_len[12..16].copy_from_slice(&u32::MAX.to_le_bytes());
    assert_eq!(
        ClipPayload::decode(b"garbage"),
        Err(ClipboardError::NotFerrum)
    );
    assert_eq!(
        ClipPayload::decode(&good[..7]),
        Err(ClipboardError::NotFerrum)
    );
    assert_eq!(
        ClipPayload::decode(&newer),
        Err(ClipboardError::NewerVersion(VERSION + 1))
    );
    for (bytes, what) in [
        (good[..good.len() - 1].to_vec(), "обрыв"),
        ([good.clone(), vec![0]].concat(), "лишние байты"),
        (count, "число изображений"),
        (huge_len, "длина JSON за концом"),
        (with_json("{\"nodes\":1}"), "не тот JSON"),
    ] {
        assert!(
            matches!(
                ClipPayload::decode(&bytes),
                Err(ClipboardError::Corrupted(_))
            ),
            "{what}"
        );
    }
}

#[test]
fn hostile_values_are_rejected_or_clamped() {
    // Без байтов изображений: `with_json` пишет пустой список картинок.
    let mut base = sample();
    base.images.clear();
    let json = json_of(&base.encode());
    let bad = |from: &str, to: &str| {
        assert!(json.contains(from), "{from}");
        ClipPayload::decode(&with_json(&json.replacen(from, to, 1)))
    };
    let corrupt =
        |r: Result<ClipPayload, ClipboardError>| matches!(r, Err(ClipboardError::Corrupted(_)));
    assert!(
        corrupt(bad("\"parent\":0", "\"parent\":2")),
        "ссылка вперёд"
    );
    assert!(
        corrupt(bad("\"parent\":0", "\"parent\":1")),
        "ссылка на себя"
    );
    assert!(corrupt(bad("#ffffffff", "#nothex")), "цвет");
    assert!(corrupt(bad(
        "\"scaleMode\":\"fill\"",
        "\"scaleMode\":\"stretch\""
    )));
    assert!(corrupt(bad(
        "\"align\":\"outside\"",
        "\"align\":\"middle\""
    )));
    assert!(corrupt(bad(&"ab".repeat(32), "../../etc/passwd")), "хэш");
    assert!(
        corrupt(bad("\"bounds\":[", "\"bounds\":[1e999,")),
        "границы"
    );
    // Огромные числа приводятся к пределам сцены, линия остаётся без высоты.
    let p =
        ClipPayload::decode(&with_json(&json.replacen("\"x\":10.0", "\"x\":1e300", 1))).unwrap();
    assert_eq!(p.nodes[0].props.x, ferrum_scene::shape::COORD_LIMIT);
    let p = ClipPayload::decode(&with_json(&json.replace("\"h\":0.0", "\"h\":50.0"))).unwrap();
    assert_eq!(p.nodes[2].props.h, 0.0);
    // Длинное имя усекается.
    let long = "я".repeat(5000);
    let p = ClipPayload::decode(&with_json(&json.replacen(
        "\"Фрейм\"",
        &format!("\"{long}\""),
        1,
    )))
    .unwrap();
    assert_eq!(p.nodes[0].props.name.chars().count(), MAX_NAME);
    // Неизвестный тип пропускается вместе с поддеревом, индексы пересчитываются.
    let p = ClipPayload::decode(&with_json(&json.replacen(
        "\"kind\":\"frame\"",
        "\"kind\":\"star\"",
        1,
    )))
    .unwrap();
    assert_eq!(p.nodes.len(), 1);
    assert_eq!(p.nodes[0].props.kind, NodeKind::Line);
    // Ничего не осталось — нечего вставлять.
    assert_eq!(
        ClipPayload::decode(&with_json(
            "{\"nodes\":[],\"images\":[],\"bounds\":[0,0,1,1]}"
        )),
        Err(ClipboardError::Empty)
    );
}

#[test]
fn limits_are_enforced() {
    let mut p = sample();
    p.nodes[1].props.fills = vec![Fill::solid(Color::BLACK); MAX_PAINTS + 1];
    assert!(matches!(
        ClipPayload::decode(&p.encode()),
        Err(ClipboardError::TooLarge(_))
    ));
    let big = vec![0u8; MAX_PAYLOAD + 1];
    assert!(matches!(
        ClipPayload::decode(&big),
        Err(ClipboardError::TooLarge(_))
    ));
}
