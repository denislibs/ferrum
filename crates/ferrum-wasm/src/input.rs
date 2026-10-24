//! Записи кольцевого буфера ввода: кодирование и разбор. Основной поток пишет те же
//! байты из TypeScript (`apps/web/src/core/input.ts`), worker разбирает их здесь.
//!
//! Данные из общей памяти не доверенные: другой поток мог записать что угодно.
//! Разбор не паникует ни на каких байтах — непонятная запись пропускается
//! целиком и учитывается в `Decoded::skipped`.

use ferrum_editor::{Input, KeyEvent, Modifiers, PointerButton, PointerEvent, WheelEvent};
use kurbo::{Point, Vec2};

use crate::layout::{field, flag, kind, RECORD_SIZE};

/// Наибольшая сторона холста в CSS-пикселях, которую принимает запись размера.
pub const MAX_VIEWPORT_SIDE: f64 = 100_000.0;
/// Наибольший `devicePixelRatio`.
pub const MAX_DPR: f64 = 16.0;

/// Одна запись буфера: событие редактора или новый размер холста.
#[derive(Debug, Clone, PartialEq)]
pub enum Record {
    Input(Input),
    /// Размер холста в CSS-пикселях и `devicePixelRatio` (`ResizeObserver`).
    Resize {
        width: f64,
        height: f64,
        dpr: f64,
    },
}

/// Результат разбора пачки записей.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct Decoded {
    pub records: Vec<Record>,
    /// Пропущено непонятных записей (и неполный хвост, если длина не кратна
    /// `RECORD_SIZE`).
    pub skipped: usize,
}

fn put_f64(out: &mut [u8; RECORD_SIZE], at: usize, v: f64) {
    out[at..at + 8].copy_from_slice(&v.to_le_bytes());
}

fn get_f64(rec: &[u8], at: usize) -> f64 {
    f64::from_le_bytes(rec[at..at + 8].try_into().expect("8 байт"))
}

fn mods_flags(m: Modifiers) -> u8 {
    let mut f = 0;
    if m.shift {
        f |= flag::SHIFT;
    }
    if m.alt {
        f |= flag::ALT;
    }
    if m.ctrl {
        f |= flag::CTRL;
    }
    if m.meta {
        f |= flag::META;
    }
    f
}

fn flags_mods(f: u8) -> Modifiers {
    Modifiers {
        shift: f & flag::SHIFT != 0,
        alt: f & flag::ALT != 0,
        ctrl: f & flag::CTRL != 0,
        meta: f & flag::META != 0,
    }
}

/// Обрезает строку до `TEXT_MAX` байт по границе символа.
fn clip(s: &str) -> &str {
    if s.len() <= field::TEXT_MAX {
        return s;
    }
    let mut end = field::TEXT_MAX;
    while !s.is_char_boundary(end) {
        end -= 1;
    }
    &s[..end]
}

fn put_text(out: &mut [u8; RECORD_SIZE], len_at: usize, at: usize, s: &str) {
    let s = clip(s);
    out[len_at] = s.len() as u8;
    out[at..at + s.len()].copy_from_slice(s.as_bytes());
}

fn get_text(rec: &[u8], len_at: usize, at: usize) -> Option<String> {
    let len = rec[len_at] as usize;
    if len > field::TEXT_MAX {
        return None;
    }
    std::str::from_utf8(&rec[at..at + len])
        .ok()
        .map(str::to_string)
}

fn button_code(b: PointerButton) -> u8 {
    match b {
        PointerButton::Primary => 0,
        PointerButton::Middle => 1,
        PointerButton::Secondary => 2,
    }
}

/// Кодирует запись. Строки клавиш длиннее `TEXT_MAX` байт обрезаются по границе
/// символа (настоящие `KeyboardEvent.code` не длиннее 20 символов ASCII).
pub fn encode(record: &Record) -> [u8; RECORD_SIZE] {
    let mut out = [0u8; RECORD_SIZE];
    let pointer = |out: &mut [u8; RECORD_SIZE], k: u8, e: &PointerEvent| {
        out[field::KIND] = k;
        out[field::BUTTON] = button_code(e.button);
        out[field::FLAGS] = mods_flags(e.modifiers);
        put_f64(out, field::A, e.point.x);
        put_f64(out, field::B, e.point.y);
        put_f64(out, field::C, e.time_ms);
    };
    let key = |out: &mut [u8; RECORD_SIZE], k: u8, e: &KeyEvent| {
        out[field::KIND] = k;
        let mut f = mods_flags(e.modifiers);
        if e.alt_graph {
            f |= flag::ALT_GRAPH;
        }
        if e.repeat {
            f |= flag::REPEAT;
        }
        out[field::FLAGS] = f;
        put_text(out, field::CODE_LEN, field::CODE, &e.code);
        put_text(out, field::KEY_LEN, field::KEY, &e.key);
    };
    match record {
        Record::Input(Input::PointerDown(e)) => pointer(&mut out, kind::POINTER_DOWN, e),
        Record::Input(Input::PointerMove(e)) => pointer(&mut out, kind::POINTER_MOVE, e),
        Record::Input(Input::PointerUp(e)) => pointer(&mut out, kind::POINTER_UP, e),
        Record::Input(Input::PointerCancel) => out[field::KIND] = kind::POINTER_CANCEL,
        Record::Input(Input::Wheel(e)) => {
            out[field::KIND] = kind::WHEEL;
            out[field::FLAGS] = mods_flags(e.modifiers);
            put_f64(&mut out, field::A, e.point.x);
            put_f64(&mut out, field::B, e.point.y);
            put_f64(&mut out, field::C, e.delta.x);
            put_f64(&mut out, field::D, e.delta.y);
        }
        Record::Input(Input::Pinch { point, scale }) => {
            out[field::KIND] = kind::PINCH;
            put_f64(&mut out, field::A, point.x);
            put_f64(&mut out, field::B, point.y);
            put_f64(&mut out, field::C, *scale);
        }
        Record::Input(Input::KeyDown(e)) => key(&mut out, kind::KEY_DOWN, e),
        Record::Input(Input::KeyUp(e)) => key(&mut out, kind::KEY_UP, e),
        Record::Input(Input::Blur) => out[field::KIND] = kind::BLUR,
        Record::Resize { width, height, dpr } => {
            out[field::KIND] = kind::RESIZE;
            put_f64(&mut out, field::A, *width);
            put_f64(&mut out, field::B, *height);
            put_f64(&mut out, field::C, *dpr);
        }
    }
    out
}

/// Разбирает одну запись. `None` — запись непонятна (неизвестный вид, кнопка,
/// строка не UTF-8, недопустимый размер) или длина не `RECORD_SIZE`.
///
/// Нечисловые координаты указателя не отбрасываются здесь: их отбрасывает сам
/// редактор (`Editor::handle`), одинаково для любого источника событий.
pub fn decode(rec: &[u8]) -> Option<Record> {
    if rec.len() != RECORD_SIZE {
        return None;
    }
    let flags = rec[field::FLAGS];
    let modifiers = flags_mods(flags);
    let point = || Point::new(get_f64(rec, field::A), get_f64(rec, field::B));
    let pointer = |button: bool| -> Option<PointerEvent> {
        let button = if button {
            match rec[field::BUTTON] {
                0 => PointerButton::Primary,
                1 => PointerButton::Middle,
                2 => PointerButton::Secondary,
                _ => return None,
            }
        } else {
            PointerButton::Primary
        };
        Some(PointerEvent {
            point: point(),
            button,
            modifiers,
            time_ms: get_f64(rec, field::C),
        })
    };
    let key = || -> Option<KeyEvent> {
        let code = get_text(rec, field::CODE_LEN, field::CODE)?;
        if code.is_empty() {
            return None;
        }
        Some(KeyEvent {
            code,
            key: get_text(rec, field::KEY_LEN, field::KEY)?,
            modifiers,
            repeat: flags & flag::REPEAT != 0,
            alt_graph: flags & flag::ALT_GRAPH != 0,
        })
    };
    let input = match rec[field::KIND] {
        kind::POINTER_DOWN => Input::PointerDown(pointer(true)?),
        kind::POINTER_MOVE => Input::PointerMove(pointer(false)?),
        kind::POINTER_UP => Input::PointerUp(pointer(true)?),
        kind::POINTER_CANCEL => Input::PointerCancel,
        kind::WHEEL => Input::Wheel(WheelEvent {
            point: point(),
            delta: Vec2::new(get_f64(rec, field::C), get_f64(rec, field::D)),
            modifiers,
        }),
        kind::PINCH => Input::Pinch {
            point: point(),
            scale: get_f64(rec, field::C),
        },
        kind::KEY_DOWN => Input::KeyDown(key()?),
        kind::KEY_UP => Input::KeyUp(key()?),
        kind::BLUR => Input::Blur,
        kind::RESIZE => {
            let (width, height, dpr) = (
                get_f64(rec, field::A),
                get_f64(rec, field::B),
                get_f64(rec, field::C),
            );
            let side = |v: f64| v.is_finite() && v > 0.0 && v <= MAX_VIEWPORT_SIDE;
            if !(side(width) && side(height) && dpr.is_finite() && dpr > 0.0 && dpr <= MAX_DPR) {
                return None;
            }
            return Some(Record::Resize { width, height, dpr });
        }
        _ => return None,
    };
    Some(Record::Input(input))
}

/// Разбирает подряд идущие записи. Неполный хвост считается одной пропущенной
/// записью.
pub fn decode_all(bytes: &[u8]) -> Decoded {
    let mut out = Decoded::default();
    let chunks = bytes.chunks_exact(RECORD_SIZE);
    if !chunks.remainder().is_empty() {
        out.skipped += 1;
    }
    for rec in chunks {
        match decode(rec) {
            Some(r) => out.records.push(r),
            None => out.skipped += 1,
        }
    }
    out
}
