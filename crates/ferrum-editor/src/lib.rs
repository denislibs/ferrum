//! Редактор Ferrum без браузера: инструменты, выделение, привязки, команды и
//! горячие клавиши, буфер обмена. Получает события ввода простыми структурами,
//! правит документ транзакциями `ferrum-doc`, держит сцену с превью жеста, камеру и
//! оверлей для `ferrum-render`. Не зависит ни от браузера, ни от ОС и детерминирован:
//! одна и та же последовательность событий даёт один и тот же документ.

pub mod clipboard;
pub mod commands;
pub mod geometry;
pub mod input;
pub mod keymap;
pub mod presets;
pub mod selection;
pub mod snap;

pub use commands::{Command, CommandId, COMMANDS};
pub use input::{Input, KeyEvent, Modifiers, Platform, PointerButton, PointerEvent, WheelEvent};
pub use keymap::{browser_reserved, Conflict, Keymap, KeymapError, Shortcut};
pub use presets::{frame_presets, parse_presets, FramePreset, PresetError};
