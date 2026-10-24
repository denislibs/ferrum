//! События ввода — простые структуры без браузера. Основной поток (этап 5) кладёт
//! их в кольцевой буфер как есть, worker отдаёт редактору по порядку. Координаты —
//! CSS-пиксели вьюпорта холста, время — миллисекунды (`event.timeStamp`).

use kurbo::{Point, Vec2};
use serde::{Deserialize, Serialize};

/// Платформа определяет, какая клавиша главная в сочетаниях: Cmd на macOS, Ctrl на
/// Windows и Linux.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Platform {
    Mac,
    Windows,
    Linux,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct Modifiers {
    pub shift: bool,
    pub alt: bool,
    pub ctrl: bool,
    pub meta: bool,
}

impl Modifiers {
    pub const NONE: Modifiers = Modifiers {
        shift: false,
        alt: false,
        ctrl: false,
        meta: false,
    };
    pub const SHIFT: Modifiers = Modifiers {
        shift: true,
        ..Modifiers::NONE
    };
    pub const ALT: Modifiers = Modifiers {
        alt: true,
        ..Modifiers::NONE
    };
    pub const CTRL: Modifiers = Modifiers {
        ctrl: true,
        ..Modifiers::NONE
    };
    pub const META: Modifiers = Modifiers {
        meta: true,
        ..Modifiers::NONE
    };

    /// Главная клавиша сочетаний на платформе: Cmd на macOS, Ctrl на остальных.
    pub fn primary(self, platform: Platform) -> bool {
        match platform {
            Platform::Mac => self.meta,
            _ => self.ctrl,
        }
    }

    pub fn with_shift(self) -> Modifiers {
        Modifiers {
            shift: true,
            ..self
        }
    }

    pub fn with_alt(self) -> Modifiers {
        Modifiers { alt: true, ..self }
    }
}

/// Кнопка мыши, нажатая или отпущенная в событии.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PointerButton {
    Primary,
    Middle,
    Secondary,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PointerEvent {
    /// Точка в CSS-пикселях относительно левого верхнего угла холста.
    pub point: Point,
    pub button: PointerButton,
    pub modifiers: Modifiers,
    /// Время события в миллисекундах; по нему распознаётся двойной клик.
    pub time_ms: f64,
}

impl PointerEvent {
    pub fn new(point: impl Into<Point>, modifiers: Modifiers, time_ms: f64) -> PointerEvent {
        PointerEvent {
            point: point.into(),
            button: PointerButton::Primary,
            modifiers,
            time_ms,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WheelEvent {
    pub point: Point,
    /// Прокрутка в CSS-пикселях (`deltaMode` уже приведён основным потоком).
    pub delta: Vec2,
    pub modifiers: Modifiers,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyEvent {
    /// Физическая клавиша, как `KeyboardEvent.code` («KeyV», «Digit1», «BracketLeft»):
    /// по ней сопоставляются сочетания, поэтому они работают на любой раскладке.
    pub code: String,
    /// Символ на текущей раскладке, как `KeyboardEvent.key`. Только для подписей.
    pub key: String,
    pub modifiers: Modifiers,
    /// Автоповтор удерживаемой клавиши.
    pub repeat: bool,
    /// Нажат AltGr (`getModifierState("AltGraph")`). На Windows он приходит как
    /// Ctrl+Alt, поэтому с ним сочетания с Mod+Alt не срабатывают: пользователь
    /// набирает символ, а не вызывает команду.
    pub alt_graph: bool,
}

impl KeyEvent {
    pub fn new(code: &str, modifiers: Modifiers) -> KeyEvent {
        KeyEvent {
            code: code.to_string(),
            key: String::new(),
            modifiers,
            repeat: false,
            alt_graph: false,
        }
    }
}

/// Одно событие ввода. Текстовый ввод — в подсистеме 3.
#[derive(Debug, Clone, PartialEq)]
pub enum Input {
    PointerDown(PointerEvent),
    PointerMove(PointerEvent),
    PointerUp(PointerEvent),
    /// Указатель потерян (`pointercancel`): жест отменяется.
    PointerCancel,
    Wheel(WheelEvent),
    /// Щипок тачпада в Safari (`gesturechange`): множитель зума к точке.
    Pinch {
        point: Point,
        scale: f64,
    },
    KeyDown(KeyEvent),
    KeyUp(KeyEvent),
    /// Окно потеряло фокус: отпущенных клавиш и кнопок мы уже не увидим.
    Blur,
}
