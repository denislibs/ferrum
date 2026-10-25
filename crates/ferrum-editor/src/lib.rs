//! Редактор Ferrum без браузера: инструменты, выделение, привязки, команды и
//! горячие клавиши, буфер обмена. Получает события ввода простыми структурами,
//! правит документ транзакциями `ferrum-doc`, держит сцену с превью жеста, камеру и
//! оверлей для `ferrum-render`. Не зависит ни от браузера, ни от ОС и детерминирован:
//! одна и та же последовательность событий даёт один и тот же документ.

mod actions;
pub mod clipboard;
pub mod commands;
mod editor;
pub mod geometry;
pub mod input;
pub mod keymap;
mod ops;
pub mod pages;
pub mod presets;
pub mod selection;
pub mod snap;
pub mod tools;

pub use actions::{FIT_PADDING_PX, PRESET_GAP};
pub use commands::{Command, CommandId, COMMANDS};
pub use editor::{Editor, EditorEvent, Settings, WHEEL_ZOOM_SPEED};
pub use input::{Input, KeyEvent, Modifiers, Platform, PointerButton, PointerEvent, WheelEvent};
pub use keymap::{browser_reserved, BindError, Conflict, Keymap, KeymapError, Shortcut};
pub use pages::{clean_name, MAX_NAME_CHARS};
pub use presets::{frame_presets, parse_presets, FramePreset, PresetError};
pub use tools::{Cursor, Tool, ToolKind, ToolOverlay};
