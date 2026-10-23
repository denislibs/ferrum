use ferrum_doc::{Color, NodeKind, Props};

#[test]
fn color_parses_rgb_and_rgba() {
    assert_eq!(
        Color::parse("#FF8000").unwrap(),
        Color::rgba(255, 128, 0, 255)
    );
    assert_eq!(
        Color::parse("#ff800080").unwrap(),
        Color::rgba(255, 128, 0, 128)
    );
}

#[test]
fn color_rejects_garbage() {
    for bad in ["ff8000", "#ff80", "#gg8000", "#ff8000ff00", "#ффф000"] {
        assert!(Color::parse(bad).is_err(), "{bad} должен быть отклонён");
    }
}

#[test]
fn color_hex_roundtrip_is_lowercase_rgba() {
    let c = Color::rgba(0xAB, 0x01, 0xFF, 0x7F);
    assert_eq!(c.to_hex(), "#ab01ff7f");
    assert_eq!(Color::parse(&c.to_hex()).unwrap(), c);
}

#[test]
fn node_kind_roundtrip() {
    for k in [
        NodeKind::Frame,
        NodeKind::Group,
        NodeKind::Rect,
        NodeKind::Ellipse,
        NodeKind::Line,
        NodeKind::Image,
    ] {
        assert_eq!(NodeKind::parse(k.as_str()), Some(k));
    }
    assert_eq!(NodeKind::parse("star"), None);
}

#[test]
fn defaults_follow_figma() {
    let frame = Props::defaults(NodeKind::Frame);
    assert!(frame.clip_content);
    assert_eq!(frame.fills[0].color, Color::WHITE);
    let rect = Props::defaults(NodeKind::Rect);
    assert_eq!(rect.fills[0].color, Color::SHAPE_GRAY);
    assert!(!rect.clip_content);
    let line = Props::defaults(NodeKind::Line);
    assert!(line.fills.is_empty());
    assert_eq!(line.strokes.len(), 1);
    assert_eq!(line.h, 0.0);
}
