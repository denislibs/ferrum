//! Формат буфера обмена Ferrum: кодирование и разбор не доверенных данных.

use ferrum_doc::{Color, Fill, NodeKind, Props, Stroke, StrokeAlign};
use ferrum_editor::clipboard::*;
use ferrum_scene::shape::COORD_LIMIT;
use kurbo::Rect;

/// Хэш blake3 в нижнем регистре, как его пишет редактор.
fn hash_of(bytes: &[u8]) -> String {
    blake3::hash(bytes).to_hex().to_string()
}

const SAMPLE_IMAGE: &[u8] = &[1, 2, 3];

fn sample() -> ClipPayload {
    let hash = hash_of(SAMPLE_IMAGE);
    let mut frame = Props::defaults(NodeKind::Frame);
    frame.x = 10.0;
    frame.rotation = 15.0;
    let mut pic = Props::defaults(NodeKind::Image);
    pic.fills = vec![Fill::image(hash.clone())];
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
        images: vec![(hash, SAMPLE_IMAGE.to_vec())],
        bounds: Rect::new(10.0, 0.0, 110.0, 100.0),
    }
}

/// Байты с заданным JSON и телами изображений (длины пишутся по фактическим телам).
fn frame_bytes(json: &str, images: &[Vec<u8>]) -> Vec<u8> {
    let mut out = MAGIC.to_vec();
    out.extend_from_slice(&VERSION.to_le_bytes());
    out.extend_from_slice(&(json.len() as u32).to_le_bytes());
    out.extend_from_slice(json.as_bytes());
    out.extend_from_slice(&(images.len() as u32).to_le_bytes());
    for img in images {
        out.extend_from_slice(&(img.len() as u32).to_le_bytes());
        out.extend_from_slice(img);
    }
    out
}

/// Байты с заменённым JSON (без изображений).
fn with_json(json: &str) -> Vec<u8> {
    frame_bytes(json, &[])
}

fn json_of(bytes: &[u8]) -> String {
    let len = u32::from_le_bytes(bytes[12..16].try_into().unwrap()) as usize;
    String::from_utf8(bytes[16..16 + len].to_vec()).unwrap()
}

/// JSON одного узла со всеми полями.
fn node_json(parent: Option<usize>, kind: &str) -> String {
    let parent = parent.map_or("null".to_string(), |p| p.to_string());
    format!(
        "{{\"parent\":{parent},\"kind\":\"{kind}\",\"name\":\"n\",\"x\":0.0,\"y\":0.0,\
\"w\":10.0,\"h\":10.0,\"rotation\":0.0,\"opacity\":1.0,\"visible\":true,\"locked\":false,\
\"cornerRadius\":0.0,\"clipContent\":false,\"fills\":[],\"strokes\":[]}}"
    )
}

/// JSON документа из готовых узлов, хэшей и рамки.
fn doc_json(nodes: &[String], images: &[String], bounds: &str) -> String {
    let images: Vec<String> = images.iter().map(|h| format!("\"{h}\"")).collect();
    format!(
        "{{\"nodes\":[{}],\"images\":[{}],\"bounds\":{bounds}}}",
        nodes.join(","),
        images.join(",")
    )
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
    let mut zero = good.clone();
    zero[8..12].copy_from_slice(&0u32.to_le_bytes());
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
    assert!(
        matches!(
            ClipPayload::decode(&zero),
            Err(ClipboardError::Corrupted(_))
        ),
        "версия 0"
    );
    assert!(
        matches!(
            ClipPayload::decode(&huge_len),
            Err(ClipboardError::TooLarge(_))
        ),
        "длина JSON сверх предела"
    );
    for (bytes, what) in [
        (good[..good.len() - 1].to_vec(), "обрыв"),
        ([good.clone(), vec![0]].concat(), "лишние байты"),
        (count, "число изображений"),
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
fn every_prefix_is_rejected_without_panic() {
    let good = sample().encode();
    for n in 0..good.len() {
        assert!(ClipPayload::decode(&good[..n]).is_err(), "префикс {n}");
    }
}

#[test]
fn byte_flips_never_panic_and_accepted_results_validate() {
    let good = sample().encode();
    for i in 0..good.len() {
        for mask in [0x01u8, 0x40, 0x80, 0xFF] {
            let mut bytes = good.clone();
            bytes[i] ^= mask;
            if let Ok(p) = ClipPayload::decode(&bytes) {
                // Принятый результат снова кодируется и разбирается в тот же.
                assert_eq!(ClipPayload::decode(&p.encode()).unwrap(), p, "байт {i}");
            }
        }
    }
}

#[test]
fn garbage_in_the_middle_is_rejected() {
    let good = sample().encode();
    let mid = 16 + json_of(&good).len() / 2;
    let mut bytes = good.clone();
    bytes[mid..mid + 4].copy_from_slice(&[0xFF, 0xFE, 0xFD, 0xFC]);
    assert!(matches!(
        ClipPayload::decode(&bytes),
        Err(ClipboardError::Corrupted(_))
    ));
}

#[test]
fn hostile_values_are_rejected_or_clamped() {
    let base = sample();
    let json = json_of(&base.encode());
    let images = vec![SAMPLE_IMAGE.to_vec()];
    let hash = hash_of(SAMPLE_IMAGE);
    let bad = |from: &str, to: &str| {
        assert!(json.contains(from), "{from}");
        ClipPayload::decode(&frame_bytes(&json.replacen(from, to, 1), &images))
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
    assert!(corrupt(bad(&hash, "../../etc/passwd")), "хэш");
    assert!(
        corrupt(bad("\"bounds\":[", "\"bounds\":[1e999,")),
        "границы"
    );
    // Огромные числа приводятся к пределам сцены, линия остаётся без высоты.
    let p = ClipPayload::decode(&frame_bytes(
        &json.replacen("\"x\":10.0", "\"x\":1e300", 1),
        &images,
    ))
    .unwrap();
    assert_eq!(p.nodes[0].props.x, COORD_LIMIT);
    let p = ClipPayload::decode(&frame_bytes(
        &json.replace("\"h\":0.0", "\"h\":50.0"),
        &images,
    ))
    .unwrap();
    assert_eq!(p.nodes[2].props.h, 0.0);
    // Длинное имя усекается.
    let long = "я".repeat(5000);
    let p = ClipPayload::decode(&frame_bytes(
        &json.replacen("\"Фрейм\"", &format!("\"{long}\""), 1),
        &images,
    ))
    .unwrap();
    assert_eq!(p.nodes[0].props.name.chars().count(), MAX_NAME);
    // Неизвестный тип пропускается вместе с поддеревом, индексы пересчитываются.
    let p = ClipPayload::decode(&frame_bytes(
        &json.replacen("\"kind\":\"frame\"", "\"kind\":\"star\"", 1),
        &images,
    ))
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
fn image_bytes_must_match_their_hash() {
    let mut p = sample();
    p.images[0].0 = "ab".repeat(32);
    p.nodes[1].props.fills[0].image_hash = Some("ab".repeat(32));
    assert!(matches!(
        ClipPayload::decode(&p.encode()),
        Err(ClipboardError::Corrupted(_))
    ));
}

#[test]
fn uppercase_hash_is_accepted_and_normalised() {
    let mut p = sample();
    let upper = hash_of(SAMPLE_IMAGE).to_uppercase();
    p.images[0].0 = upper.clone();
    p.nodes[1].props.fills[0].image_hash = Some(upper);
    let q = ClipPayload::decode(&p.encode()).unwrap();
    let lower = hash_of(SAMPLE_IMAGE);
    assert_eq!(q.images[0].0, lower);
    assert_eq!(
        q.nodes[1].props.fills[0].image_hash.as_deref(),
        Some(lower.as_str())
    );
}

#[test]
fn duplicate_image_hashes_are_rejected() {
    let mut p = sample();
    let dup = p.images[0].clone();
    p.images.push(dup);
    assert!(matches!(
        ClipPayload::decode(&p.encode()),
        Err(ClipboardError::Corrupted(_))
    ));
}

#[test]
fn fill_referencing_missing_image_is_rejected() {
    let mut p = sample();
    p.images.clear();
    assert!(matches!(
        ClipPayload::decode(&p.encode()),
        Err(ClipboardError::Corrupted(_))
    ));
}

#[test]
fn only_containers_can_have_children() {
    let json = doc_json(
        &[node_json(None, "rect"), node_json(Some(0), "frame")],
        &[],
        "[0,0,1,1]",
    );
    assert!(matches!(
        ClipPayload::decode(&with_json(&json)),
        Err(ClipboardError::Corrupted(_))
    ));
}

#[test]
fn depth_is_limited() {
    let chain = |len: usize| {
        let nodes: Vec<String> = (0..len)
            .map(|i| node_json(i.checked_sub(1), "frame"))
            .collect();
        doc_json(&nodes, &[], "[0,0,1,1]")
    };
    assert!(ClipPayload::decode(&with_json(&chain(MAX_DEPTH + 1))).is_ok());
    assert!(matches!(
        ClipPayload::decode(&with_json(&chain(MAX_DEPTH + 2))),
        Err(ClipboardError::TooLarge(_))
    ));
}

#[test]
fn bounds_are_clamped_to_scene_limit() {
    let json = doc_json(
        &[node_json(None, "frame")],
        &[],
        "[-1e308,-1e308,1e308,1e308]",
    );
    let p = ClipPayload::decode(&with_json(&json)).unwrap();
    assert_eq!(
        p.bounds,
        Rect::new(-COORD_LIMIT, -COORD_LIMIT, COORD_LIMIT, COORD_LIMIT)
    );
}

#[test]
fn declared_sizes_are_capped_before_copying() {
    let good = sample().encode();
    // Длина JSON сверх предела: отказ без разбора.
    let mut big_json = good.clone();
    big_json[12..16].copy_from_slice(&((MAX_JSON + 1) as u32).to_le_bytes());
    assert!(matches!(
        ClipPayload::decode(&big_json),
        Err(ClipboardError::TooLarge(_))
    ));
    // Длина изображения почти 4 ГБ при коротком теле: отказ без копирования.
    let at = 16 + json_of(&good).len() + 4;
    let mut big_image = good.clone();
    big_image[at..at + 4].copy_from_slice(&(u32::MAX - 1).to_le_bytes());
    assert!(matches!(
        ClipPayload::decode(&big_image),
        Err(ClipboardError::TooLarge(_))
    ));
}

#[test]
fn node_and_image_counts_are_limited() {
    let nodes: Vec<String> = (0..=MAX_NODES).map(|_| node_json(None, "group")).collect();
    assert!(matches!(
        ClipPayload::decode(&with_json(&doc_json(&nodes, &[], "[0,0,1,1]"))),
        Err(ClipboardError::TooLarge(_))
    ));
    let hashes: Vec<String> = (0..=MAX_IMAGES).map(|i| format!("{i:064x}")).collect();
    let bytes = frame_bytes(
        &doc_json(&[], &hashes, "[0,0,1,1]"),
        &vec![Vec::new(); MAX_IMAGES + 1],
    );
    assert!(matches!(
        ClipPayload::decode(&bytes),
        Err(ClipboardError::TooLarge(_))
    ));
}

#[test]
fn encodable_refuses_what_paste_would_refuse() {
    assert!(encodable(&sample()).is_ok());
    let mut long = sample();
    long.nodes[0].props.name = "я".repeat(MAX_NAME + 1);
    assert!(matches!(encodable(&long), Err(ClipboardError::TooLarge(_))));
    let mut paints = sample();
    paints.nodes[1].props.fills = vec![Fill::solid(Color::BLACK); MAX_PAINTS + 1];
    assert!(matches!(
        encodable(&paints),
        Err(ClipboardError::TooLarge(_))
    ));
    let mut missing = sample();
    missing.images.clear();
    assert!(matches!(
        encodable(&missing),
        Err(ClipboardError::Corrupted(_))
    ));
    let empty = ClipPayload {
        nodes: vec![],
        images: vec![],
        bounds: Rect::new(0.0, 0.0, 1.0, 1.0),
    };
    assert_eq!(encodable(&empty), Err(ClipboardError::Empty));
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

#[test]
fn flips_travel_in_props_and_old_payloads_decode_unmirrored() {
    let mut p = sample();
    p.nodes[0].props.flip_x = true;
    p.nodes[1].props.flip_y = true;
    let bytes = p.encode();
    assert_eq!(ClipPayload::decode(&bytes).unwrap(), p);
    let json = json_of(&bytes);
    assert!(json.contains("\"flipX\":true"), "{json}");
    assert!(json.contains("\"flipY\":true"), "{json}");
    // Неотражённый узел пишется как раньше, без новых полей: старая сборка
    // разберёт такую копию без потерь.
    let plain = sample().encode();
    assert!(!json_of(&plain).contains("flip"), "{}", json_of(&plain));
    assert_eq!(VERSION, 1, "формат не менялся: поля необязательные");
    // Копия из прежней версии (полный узел без полей отражения) — без отражения.
    let old = format!(
        "{{\"nodes\":[{}],\"images\":[],\"bounds\":[0.0,0.0,10.0,10.0]}}",
        node_json(None, "rect")
    );
    let decoded = ClipPayload::decode(&with_json(&old)).unwrap();
    assert!(!decoded.nodes[0].props.flip_x && !decoded.nodes[0].props.flip_y);
    // Не логическое значение — повреждённые данные, а не молчаливое `false`.
    let bad = old.replace("\"clipContent\"", "\"flipX\":\"да\",\"clipContent\"");
    assert!(matches!(
        ClipPayload::decode(&with_json(&bad)),
        Err(ClipboardError::Corrupted(_))
    ));
}

#[test]
fn hostile_group_size_is_dropped() {
    // Группа своей рамки не имеет: размер из чужих данных обнуляется, иначе её
    // отражение шло бы не относительно начала координат, а относительно выдуманного
    // центра.
    let group = node_json(None, "group")
        .replace("\"w\":10.0", "\"w\":5000.0")
        .replace("\"h\":10.0", "\"h\":-3.0")
        .replace("\"clipContent\"", "\"flipX\":true,\"clipContent\"");
    let child = node_json(Some(0), "rect");
    let json =
        format!("{{\"nodes\":[{group},{child}],\"images\":[],\"bounds\":[0.0,0.0,10.0,10.0]}}");
    let p = ClipPayload::decode(&with_json(&json)).unwrap();
    let g = &p.nodes[0].props;
    assert_eq!(g.kind, NodeKind::Group);
    assert_eq!((g.w, g.h), (0.0, 0.0));
    assert!(g.flip_x);
    assert_eq!((p.nodes[1].props.w, p.nodes[1].props.h), (10.0, 10.0));
}

#[test]
fn vector_and_stroke_style_round_trip_and_are_checked() {
    use ferrum_doc::{StrokeCap, StrokeJoin, VectorNetwork, VectorPoint, VectorSegment};
    use std::sync::Arc;
    let mut v = Props::defaults(NodeKind::Vector);
    v.w = 30.0;
    v.stroke_style.cap_end = StrokeCap::ArrowLines;
    v.stroke_style.join = StrokeJoin::Round;
    v.stroke_style.miter_limit = 10.0;
    v.vector = Some(Arc::new(VectorNetwork {
        points: vec![VectorPoint::new(0.0, 0.0), VectorPoint::new(30.0, 0.0)],
        segments: vec![VectorSegment::line(0, 1)],
        regions: vec![],
    }));
    let p = ClipPayload {
        nodes: vec![ClipNode {
            parent: None,
            props: v,
        }],
        images: vec![],
        bounds: Rect::new(0.0, 0.0, 30.0, 0.0),
    };
    let bytes = p.encode();
    let json = json_of(&bytes);
    assert!(json.contains("\"strokeCapEnd\":\"arrowLines\""), "{json}");
    assert!(
        !json.contains("strokeCapStart"),
        "нейтральное не пишется: {json}"
    );
    assert_eq!(ClipPayload::decode(&bytes).unwrap(), p);
    // Чужие данные: неизвестное окончание, предел вне диапазона, битая сеть.
    for (from, to) in [
        ("\"arrowLines\"", "\"arrow\""),
        ("\"strokeMiterLimit\":10.0", "\"strokeMiterLimit\":1000.0"),
        ("\"vector\":\"", "\"vector\":\"AAAA"),
    ] {
        let bad = json.replace(from, to);
        assert_ne!(bad, json);
        assert!(
            matches!(
                ClipPayload::decode(&with_json(&bad)),
                Err(ClipboardError::Corrupted(_))
            ),
            "{to}"
        );
    }
    // Узел без поля сети — пустой вектор, а не ошибка.
    let empty = ClipPayload::decode(&with_json(&doc_json(
        &[node_json(None, "vector")],
        &[],
        "[0.0,0.0,10.0,10.0]",
    )))
    .unwrap();
    assert_eq!(
        empty.nodes[0].props.vector.as_deref(),
        Some(&VectorNetwork::default())
    );
}

#[test]
fn boolean_with_children_round_trips_and_op_is_checked() {
    use ferrum_doc::BooleanOp;
    let mut b = Props::defaults(NodeKind::Boolean);
    b.boolean_op = Some(BooleanOp::Intersect);
    let p = ClipPayload {
        nodes: vec![
            ClipNode {
                parent: None,
                props: b,
            },
            ClipNode {
                parent: Some(0),
                props: Props::defaults(NodeKind::Rect),
            },
            ClipNode {
                parent: Some(0),
                props: Props::defaults(NodeKind::Ellipse),
            },
        ],
        images: vec![],
        bounds: Rect::new(0.0, 0.0, 100.0, 100.0),
    };
    let bytes = p.encode();
    let json = json_of(&bytes);
    assert!(json.contains("\"booleanOp\":\"intersect\""), "{json}");
    assert_eq!(ClipPayload::decode(&bytes).unwrap(), p);
    // Неизвестная операция — отказ вставки.
    let bad = json.replace("\"intersect\"", "\"divide\"");
    assert!(matches!(
        ClipPayload::decode(&with_json(&bad)),
        Err(ClipboardError::Corrupted(_))
    ));
    // Без поля — объединение; размер группы из чужих данных не принимается.
    let plain = ClipPayload::decode(&with_json(&doc_json(
        &[node_json(None, "boolean")],
        &[],
        "[0.0,0.0,10.0,10.0]",
    )))
    .unwrap();
    let props = &plain.nodes[0].props;
    assert_eq!(props.boolean_op, Some(BooleanOp::Union));
    assert_eq!((props.w, props.h), (0.0, 0.0));
    // У прямоугольника поле операции из чужих данных пропускается.
    let rect = node_json(None, "rect").replacen("\"kind\"", "\"booleanOp\":\"union\",\"kind\"", 1);
    assert!(rect.contains("\"booleanOp\":\"union\""), "{rect}");
    let foreign =
        ClipPayload::decode(&with_json(&doc_json(&[rect], &[], "[0.0,0.0,10.0,10.0]"))).unwrap();
    assert_eq!(foreign.nodes[0].props.kind, NodeKind::Rect);
    assert!(foreign.nodes[0].props.boolean_op.is_none());
}
