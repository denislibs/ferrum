//! Раскладка общей памяти: области не пересекаются и выровнены, TypeScript-файл
//! основного потока совпадает со сгенерированным.
#![cfg(not(target_arch = "wasm32"))]

use ferrum_wasm::layout::*;
use ferrum_wasm::live::{LiveField, LiveState};

#[test]
fn regions_are_aligned_and_disjoint() {
    assert_eq!(CTRL_BYTES % 8, 0);
    assert_eq!(LIVE_OFFSET, CTRL_BYTES);
    assert_eq!(LIVE_OFFSET % 8, 0, "Float64Array требует выравнивания 8");
    assert_eq!(RING_OFFSET, LIVE_OFFSET + LIVE_SLOTS * 8);
    assert_eq!(SAB_BYTES, RING_OFFSET + RING_CAPACITY * RECORD_SIZE);
    assert!(RING_CAPACITY.is_power_of_two());
    let used = [
        ctrl::FRAME,
        ctrl::INPUT_WRITE,
        ctrl::INPUT_READ,
        ctrl::LIVE_SEQ,
        ctrl::INPUT_DROPPED,
    ];
    let mut sorted = used.to_vec();
    sorted.sort();
    sorted.dedup();
    assert_eq!(sorted.len(), used.len(), "ячейки управления совпадают");
    assert!(used.iter().all(|&c| c < ctrl::SLOTS));
    // Поля записи не налезают друг на друга и на строки (остальное проверяет
    // компилятор в `layout.rs`).
    assert_eq!(field::CODE + field::TEXT_MAX, field::KEY);
    assert_eq!(field::KEY + field::TEXT_MAX, RECORD_SIZE);
}

#[test]
fn live_state_round_trips_through_slots() {
    assert!(LiveField::ALL.len() <= LIVE_SLOTS);
    for (i, f) in LiveField::ALL.iter().enumerate() {
        assert_eq!(*f as usize, i);
    }
    let s = LiveState {
        frames: 42.0,
        selection_x: -12.5,
        zoom: 0.25,
        atlas_misses: 3.0,
        ..Default::default()
    };
    let slots = s.to_slots();
    assert_eq!(slots[LiveField::FRAMES as usize], 42.0);
    assert_eq!(slots[LiveField::ATLAS_MISSES as usize], 3.0);
    assert_eq!(LiveState::from_slots(&slots), s);
    // Короткий массив (старый основной поток) — нули, без паники.
    assert_eq!(LiveState::from_slots(&slots[..2]).zoom, 0.0);
}

#[test]
fn typescript_layout_is_up_to_date() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../apps/web/src/core/layout.gen.ts");
    let expected = typescript();
    if std::env::var_os("FERRUM_UPDATE_LAYOUT").is_some() {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, &expected).unwrap();
    }
    let actual = std::fs::read_to_string(&path).unwrap_or_default();
    assert!(
        actual == expected,
        "{} устарел: FERRUM_UPDATE_LAYOUT=1 cargo test -p ferrum-wasm --test layout",
        path.display()
    );
    assert!(expected.contains("export const SAB_BYTES = 65856;"));
    assert!(expected.contains("SELECTION_X: 7,"));
}
