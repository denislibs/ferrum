//! Редактор Ferrum без браузера: инструменты, выделение, привязки, команды и
//! горячие клавиши, буфер обмена. Получает события ввода простыми структурами,
//! правит документ транзакциями `ferrum-doc`, держит сцену с превью жеста, камеру и
//! оверлей для `ferrum-render`. Не зависит ни от браузера, ни от ОС и детерминирован:
//! одна и та же последовательность событий даёт один и тот же документ.

mod actions;
mod boolean;
pub mod clipboard;
pub mod commands;
mod editor;
mod flatten;
pub mod geometry;
pub mod input;
pub mod keymap;
pub mod measure;
mod ops;
pub mod pages;
pub mod panel;
pub mod presets;
pub mod selection;
pub mod snap;
pub mod text;
mod text_clipboard;
pub mod text_edit;
mod text_input;
mod text_style;
pub mod tools;
pub mod vector_edit;
pub mod vector_ops;
mod vector_select;

pub use actions::{FIT_PADDING_PX, PRESET_GAP};
pub use commands::{Command, CommandId, COMMANDS};
pub use editor::{Editor, EditorEvent, Settings, WHEEL_ZOOM_SPEED};
pub use input::{Input, KeyEvent, Modifiers, Platform, PointerButton, PointerEvent, WheelEvent};
pub use keymap::{browser_reserved, BindError, Conflict, Keymap, KeymapError, Shortcut};
pub use pages::{clean_name, MAX_NAME_CHARS};
pub use panel::{
    display_rotation, field_applies, panel_pose, DropTarget, NumField, PanelEdit, PanelPose, Phase,
};
pub use presets::{frame_presets, parse_presets, FramePreset, PresetError};
pub use text::{fit_size, MissingFont, TextError, TextPending};
pub use text_edit::{EnterAt, TextEditInfo, CARET_BLINK_MS};
pub use text_input::TextInputKind;
pub use text_style::{Mixed, TextPropsPatch, TextStylePatch, TextStyleSummary};
pub use tools::{Cursor, Tool, ToolKind, ToolOverlay};
pub use vector_edit::{VectorSelection, VectorTool};
