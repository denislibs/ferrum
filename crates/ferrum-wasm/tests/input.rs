//! Записи кольцевого буфера ввода: каждое событие переживает кодирование и разбор,
//! а любые байты разбираются без паники.
#![cfg(not(target_arch = "wasm32"))]

use ferrum_editor::{Input, KeyEvent, Modifiers, PointerButton, PointerEvent, WheelEvent};
use ferrum_wasm::input::{decode, decode_all, encode, Record};
use ferrum_wasm::layout::{field, kind, RECORD_SIZE};
use kurbo::{Point, Vec2};
use proptest::prelude::*;

fn all_kinds() -> Vec<Record> {
    let mods = Modifiers {
        shift: true,
        alt: false,
        ctrl: true,
        meta: true,
    };
    let p = |x: f64, y: f64, button| PointerEvent {
        point: Point::new(x, y),
        button,
        modifiers: mods,
        time_ms: 1234.5,
    };
    let key = KeyEvent {
        code: "BracketLeft".into(),
        key: "х".into(),
        modifiers: Modifiers::ALT,
        repeat: true,
        alt_graph: true,
    };
    vec![
        Record::Input(Input::PointerDown(p(10.25, -3.5, PointerButton::Primary))),
        Record::Input(Input::PointerMove(p(1e7, 2e-3, PointerButton::Primary))),
        Record::Input(Input::PointerUp(p(0.0, 0.0, PointerButton::Secondary))),
        Record::Input(Input::PointerDown(p(5.0, 5.0, PointerButton::Middle))),
        Record::Input(Input::PointerCancel),
        Record::Input(Input::Wheel(WheelEvent {
            point: Point::new(100.0, 200.0),
            delta: Vec2::new(-4.0, 120.0),
            modifiers: Modifiers::CTRL,
        })),
        Record::Input(Input::Pinch {
            point: Point::new(3.0, 4.0),
            scale: 1.25,
        }),
        Record::Input(Input::KeyDown(key.clone())),
        Record::Input(Input::KeyUp(KeyEvent {
            repeat: false,
            alt_graph: false,
            ..key
        })),
        Record::Input(Input::Blur),
        Record::Resize {
            width: 1280.0,
            height: 720.0,
            dpr: 2.0,
        },
    ]
}

#[test]
fn every_kind_round_trips() {
    for rec in all_kinds() {
        let bytes = encode(&rec);
        assert_eq!(decode(&bytes), Some(rec.clone()), "{rec:?}");
    }
    // Пачкой — в том же порядке.
    let bytes: Vec<u8> = all_kinds().iter().flat_map(encode).collect();
    let d = decode_all(&bytes);
    assert_eq!((d.records, d.skipped), (all_kinds(), 0));
}

#[test]
fn layout_of_a_pointer_record_is_fixed() {
    // Те же байты пишет TypeScript: поля на своих местах, little-endian.
    let bytes = encode(&Record::Input(Input::PointerDown(PointerEvent {
        point: Point::new(1.5, 2.5),
        button: PointerButton::Secondary,
        modifiers: Modifiers::SHIFT,
        time_ms: 99.0,
    })));
    assert_eq!(bytes.len(), RECORD_SIZE);
    assert_eq!(bytes[field::KIND], kind::POINTER_DOWN);
    assert_eq!(bytes[field::BUTTON], 2);
    assert_eq!(bytes[field::FLAGS], 1);
    assert_eq!(&bytes[field::A..field::A + 8], &1.5f64.to_le_bytes());
    assert_eq!(&bytes[field::B..field::B + 8], &2.5f64.to_le_bytes());
    assert_eq!(&bytes[field::C..field::C + 8], &99.0f64.to_le_bytes());
}

#[test]
fn long_key_strings_are_clipped_on_char_boundary() {
    let rec = Record::Input(Input::KeyDown(KeyEvent {
        code: "LaunchApplication1".into(),
        key: "ыыыыыыыыыыыыыы".into(), // 28 байт: обрезается до 12 символов (24 байта)
        modifiers: Modifiers::NONE,
        repeat: false,
        alt_graph: false,
    }));
    let Some(Record::Input(Input::KeyDown(k))) = decode(&encode(&rec)) else {
        panic!("клавиша не разобралась");
    };
    assert_eq!(k.code, "LaunchApplication1");
    assert_eq!(k.key, "ы".repeat(12));
}

#[test]
fn malformed_records_are_skipped() {
    let good = encode(&Record::Input(Input::Blur));
    let mut unknown = good;
    unknown[field::KIND] = 200;
    let mut button = encode(&all_kinds()[0]);
    button[field::BUTTON] = 7;
    let mut text = encode(&all_kinds()[7]);
    text[field::CODE_LEN] = 25;
    let mut utf8 = encode(&all_kinds()[7]);
    utf8[field::CODE] = 0xff;
    let mut empty_code = encode(&all_kinds()[7]);
    empty_code[field::CODE_LEN] = 0;
    let mut bad_size = encode(&all_kinds()[10]);
    bad_size[field::C..field::C + 8].copy_from_slice(&f64::NAN.to_le_bytes());
    let mut zero_size = encode(&all_kinds()[10]);
    zero_size[field::A..field::A + 8].copy_from_slice(&0f64.to_le_bytes());
    let mut bytes = Vec::new();
    for rec in [
        unknown, button, text, utf8, empty_code, bad_size, zero_size, good,
    ] {
        bytes.extend_from_slice(&rec);
    }
    bytes.extend_from_slice(&[1, 2, 3]); // неполный хвост
    let d = decode_all(&bytes);
    assert_eq!(d.records, [Record::Input(Input::Blur)]);
    assert_eq!(d.skipped, 8);
    assert_eq!(decode(&good[..RECORD_SIZE - 1]), None);
}

proptest! {
    #[test]
    fn arbitrary_bytes_never_panic(bytes in proptest::collection::vec(any::<u8>(), 0..1024)) {
        let d = decode_all(&bytes);
        prop_assert!(d.records.len() + d.skipped >= bytes.len() / RECORD_SIZE);
    }

    #[test]
    fn arbitrary_records_never_panic(half in proptest::array::uniform32(any::<u8>()), kind in 0u8..12) {
        // Правдоподобный вид записи и мусор во всех остальных байтах.
        let mut rec = [0u8; RECORD_SIZE];
        rec[..32].copy_from_slice(&half);
        rec[32..].copy_from_slice(&half);
        rec[field::KIND] = kind;
        let _ = decode(&rec);
    }
}
