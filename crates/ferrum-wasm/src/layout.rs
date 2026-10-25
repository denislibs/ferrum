//! Раскладка общей памяти (SharedArrayBuffer) между основным потоком и worker'ом.
//!
//! ```text
//! 0               CTRL_BYTES        RING_OFFSET                       SAB_BYTES
//! | Int32 × 16    | Float64 × 32    | записи ввода × RING_CAPACITY     |
//! | управление    | живое состояние | по RECORD_SIZE байт              |
//! ```
//!
//! Управляющий блок — счётчики `Atomics`: кадр (основной поток увеличивает его в
//! `requestAnimationFrame` и будит worker `Atomics.notify`), позиции записи и чтения
//! кольцевого буфера, версия живого состояния. Кольцевой буфер — один писатель
//! (основной поток) и один читатель (worker): писатель кладёт запись в ячейку
//! `write % RING_CAPACITY` и только потом публикует `write + 1` через
//! `Atomics.store`; читатель забирает всё до `write` и публикует `read`. Счётчики
//! растут монотонно и сравниваются по модулю 2³², поэтому переполнение не страшно.
//!
//! Живое состояние пишет worker под счётчиком-«замком» (seqlock): нечётное
//! значение — идёт запись, читатель повторяет чтение, пока значение до и после не
//! совпадёт и не будет чётным.
//!
//! TypeScript-константы для основного потока генерируются отсюда
//! (`typescript()`), файл `apps/web/src/core/layout.gen.ts` сверяет тест.

use crate::live::LiveField;

/// Размер одной записи ввода в байтах.
pub const RECORD_SIZE: usize = 64;
/// Сколько записей помещается в кольцевой буфер (степень двойки). 1024 события —
/// около 16 кадров по 60 coalesced-движений указателя: столько worker не отстаёт
/// даже при открытии большого документа.
pub const RING_CAPACITY: usize = 1024;

/// Ячейки управляющего блока (индексы `Int32Array`).
pub mod ctrl {
    /// Счётчик кадров основного потока (`requestAnimationFrame`).
    pub const FRAME: usize = 0;
    /// Сколько записей ввода положено (писатель — основной поток).
    pub const INPUT_WRITE: usize = 1;
    /// Сколько записей ввода забрано (писатель — worker).
    pub const INPUT_READ: usize = 2;
    /// Версия живого состояния (seqlock: нечётная — идёт запись).
    pub const LIVE_SEQ: usize = 3;
    /// Сколько событий основной поток не смог положить: буфер был полон и
    /// очередь ожидания переполнилась.
    pub const INPUT_DROPPED: usize = 4;
    /// Ячеек в блоке.
    pub const SLOTS: usize = 16;
}

/// Байт управляющего блока.
pub const CTRL_BYTES: usize = ctrl::SLOTS * 4;
/// Начало живого состояния (`Float64Array`).
pub const LIVE_OFFSET: usize = CTRL_BYTES;
/// Ячеек живого состояния (с запасом под новые поля).
pub const LIVE_SLOTS: usize = 32;
/// Начало кольцевого буфера.
pub const RING_OFFSET: usize = LIVE_OFFSET + LIVE_SLOTS * 8;
/// Размер SharedArrayBuffer.
pub const SAB_BYTES: usize = RING_OFFSET + RING_CAPACITY * RECORD_SIZE;

/// Вид записи ввода (байт 0).
pub mod kind {
    pub const POINTER_DOWN: u8 = 1;
    pub const POINTER_MOVE: u8 = 2;
    pub const POINTER_UP: u8 = 3;
    pub const POINTER_CANCEL: u8 = 4;
    pub const WHEEL: u8 = 5;
    pub const PINCH: u8 = 6;
    pub const KEY_DOWN: u8 = 7;
    pub const KEY_UP: u8 = 8;
    pub const BLUR: u8 = 9;
    /// Размер холста: CSS-пиксели и `devicePixelRatio`.
    pub const RESIZE: u8 = 10;
}

/// Биты модификаторов (байт 2).
pub mod flag {
    pub const SHIFT: u8 = 1;
    pub const ALT: u8 = 2;
    pub const CTRL: u8 = 4;
    pub const META: u8 = 8;
    /// `KeyboardEvent.getModifierState("AltGraph")` — только настоящее состояние.
    pub const ALT_GRAPH: u8 = 16;
    pub const REPEAT: u8 = 32;
}

/// Смещения полей записи (little-endian).
///
/// | вид | a (8) | b (16) | c (24) | d (32) |
/// |---|---|---|---|---|
/// | указатель | x | y | время, мс | — |
/// | колесо | x | y | Δx | Δy |
/// | щипок | x | y | множитель | — |
/// | размер | ширина | высота | dpr | — |
///
/// У клавиш: время в `a`, `code` (UTF-8, до `TEXT_MAX` байт) с байта `CODE`,
/// `key` — с байта `KEY`; длины — в байтах `CODE_LEN` и `KEY_LEN`.
pub mod field {
    pub const KIND: usize = 0;
    /// Кнопка как `PointerEvent.button`: 0 — основная, 1 — средняя, 2 — правая.
    pub const BUTTON: usize = 1;
    pub const FLAGS: usize = 2;
    pub const CODE_LEN: usize = 3;
    pub const KEY_LEN: usize = 4;
    pub const A: usize = 8;
    pub const B: usize = 16;
    pub const C: usize = 24;
    pub const D: usize = 32;
    pub const CODE: usize = 16;
    pub const KEY: usize = 40;
    pub const TEXT_MAX: usize = 24;
}

// Раскладку проверяет компилятор: области выровнены, поля записи не налезают
// друг на друга.
const _: () = {
    assert!(LIVE_OFFSET.is_multiple_of(8));
    assert!(RING_OFFSET.is_multiple_of(8));
    assert!(RING_CAPACITY.is_power_of_two());
    assert!(field::KEY_LEN < field::A);
    assert!(field::D + 8 <= RECORD_SIZE);
    assert!(field::CODE + field::TEXT_MAX == field::KEY);
    assert!(field::KEY + field::TEXT_MAX == RECORD_SIZE);
};

/// Коды бэкендов рендера в живом состоянии.
pub const BACKENDS: [&str; 4] = ["none", "webgpu", "webgl", "cpu"];

/// Содержимое `apps/web/src/core/layout.gen.ts`.
pub fn typescript() -> String {
    let mut out = String::new();
    out.push_str("// Сгенерировано из crates/ferrum-wasm/src/layout.rs — не править руками.\n");
    out.push_str("// Обновить: FERRUM_UPDATE_LAYOUT=1 cargo test -p ferrum-wasm --test layout\n\n");
    for (name, value) in [
        ("RECORD_SIZE", RECORD_SIZE),
        ("RING_CAPACITY", RING_CAPACITY),
        ("CTRL_BYTES", CTRL_BYTES),
        ("LIVE_OFFSET", LIVE_OFFSET),
        ("LIVE_SLOTS", LIVE_SLOTS),
        ("RING_OFFSET", RING_OFFSET),
        ("SAB_BYTES", SAB_BYTES),
    ] {
        out.push_str(&format!("export const {name} = {value};\n"));
    }
    let block = |out: &mut String, name: &str, items: &[(&str, usize)]| {
        out.push_str(&format!("\nexport const {name} = {{\n"));
        for (k, v) in items {
            out.push_str(&format!("  {k}: {v},\n"));
        }
        out.push_str("} as const;\n");
    };
    block(
        &mut out,
        "Ctrl",
        &[
            ("FRAME", ctrl::FRAME),
            ("INPUT_WRITE", ctrl::INPUT_WRITE),
            ("INPUT_READ", ctrl::INPUT_READ),
            ("LIVE_SEQ", ctrl::LIVE_SEQ),
            ("INPUT_DROPPED", ctrl::INPUT_DROPPED),
        ],
    );
    block(
        &mut out,
        "Kind",
        &[
            ("POINTER_DOWN", kind::POINTER_DOWN as usize),
            ("POINTER_MOVE", kind::POINTER_MOVE as usize),
            ("POINTER_UP", kind::POINTER_UP as usize),
            ("POINTER_CANCEL", kind::POINTER_CANCEL as usize),
            ("WHEEL", kind::WHEEL as usize),
            ("PINCH", kind::PINCH as usize),
            ("KEY_DOWN", kind::KEY_DOWN as usize),
            ("KEY_UP", kind::KEY_UP as usize),
            ("BLUR", kind::BLUR as usize),
            ("RESIZE", kind::RESIZE as usize),
        ],
    );
    block(
        &mut out,
        "Flag",
        &[
            ("SHIFT", flag::SHIFT as usize),
            ("ALT", flag::ALT as usize),
            ("CTRL", flag::CTRL as usize),
            ("META", flag::META as usize),
            ("ALT_GRAPH", flag::ALT_GRAPH as usize),
            ("REPEAT", flag::REPEAT as usize),
        ],
    );
    block(
        &mut out,
        "Field",
        &[
            ("KIND", field::KIND),
            ("BUTTON", field::BUTTON),
            ("FLAGS", field::FLAGS),
            ("CODE_LEN", field::CODE_LEN),
            ("KEY_LEN", field::KEY_LEN),
            ("A", field::A),
            ("B", field::B),
            ("C", field::C),
            ("D", field::D),
            ("CODE", field::CODE),
            ("KEY", field::KEY),
            ("TEXT_MAX", field::TEXT_MAX),
        ],
    );
    let live: Vec<(&str, usize)> = LiveField::ALL
        .iter()
        .map(|f| (f.ts_name(), *f as usize))
        .collect();
    block(&mut out, "Live", &live);
    out.push_str("\nexport const LIVE_NAMES = [\n");
    for f in LiveField::ALL {
        out.push_str(&format!("  '{}',\n", f.js_name()));
    }
    out.push_str("] as const;\n");
    out.push_str("\nexport const BACKENDS = [");
    let names: Vec<String> = BACKENDS.iter().map(|b| format!("'{b}'")).collect();
    out.push_str(&names.join(", "));
    out.push_str("] as const;\n");
    out
}
