//! Единая таблица команд: меню интерфейса, палитра команд и горячие клавиши вызывают
//! одно и то же по идентификатору. Сочетания по умолчанию повторяют Figma: на macOS
//! главная клавиша — Cmd, на Windows и Linux — Ctrl (в таблице это `Mod`).

use crate::input::Platform;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum CommandId {
    ToolMove,
    ToolFrame,
    ToolRect,
    ToolEllipse,
    ToolLine,
    ToolHand,
    /// «Текст» (подсистема 3).
    ToolText,
    /// «Перо» (подсистема 3, этап 3).
    ToolPen,
    /// Рука, пока клавиша удерживается (по умолчанию Space).
    HandHold,
    ToolImage,
    Undo,
    Redo,
    Copy,
    Cut,
    Paste,
    CopyAsPng,
    Duplicate,
    Delete,
    SelectAll,
    /// Отменить жест, вернуться к «Перемещению», снять выделение — по очереди.
    Escape,
    Group,
    Ungroup,
    FrameSelection,
    BringForward,
    SendBackward,
    BringToFront,
    SendToBack,
    NudgeLeft,
    NudgeRight,
    NudgeUp,
    NudgeDown,
    NudgeLeftBig,
    NudgeRightBig,
    NudgeUpBig,
    NudgeDownBig,
    ZoomToFit,
    ZoomToSelection,
    Zoom100,
    ZoomIn,
    ZoomOut,
    ToggleVisible,
    ToggleLock,
    ToggleUi,
    /// Отразить выделение по горизонтали (слева направо) относительно центра его рамки.
    FlipHorizontal,
    /// Отразить выделение по вертикали.
    FlipVertical,
    /// Сохранить файл в хранилище (интерфейс получает `EditorEvent::SaveRequested`).
    Save,
}

/// Строка таблицы команд.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Command {
    pub id: CommandId,
    /// Стабильный ключ для настроек, меню и JSON раскладки.
    pub key: &'static str,
    /// Название в интерфейсе.
    pub name: &'static str,
    /// Сочетания по умолчанию: `Mod` — Cmd на macOS, Ctrl на остальных.
    pub defaults: &'static [&'static str],
    /// Дополнительные сочетания только для Windows и Linux.
    pub defaults_pc: &'static [&'static str],
    /// Дополнительные сочетания только для macOS.
    pub defaults_mac: &'static [&'static str],
}

const fn cmd(
    id: CommandId,
    key: &'static str,
    name: &'static str,
    defaults: &'static [&'static str],
) -> Command {
    Command {
        id,
        key,
        name,
        defaults,
        defaults_pc: &[],
        defaults_mac: &[],
    }
}

/// Команда с разными сочетаниями на macOS и на Windows/Linux.
const fn cmd_per_platform(
    id: CommandId,
    key: &'static str,
    name: &'static str,
    mac: &'static [&'static str],
    pc: &'static [&'static str],
) -> Command {
    Command {
        id,
        key,
        name,
        defaults: &[],
        defaults_pc: pc,
        defaults_mac: mac,
    }
}

use CommandId as C;

/// Все команды. Сочетания сверены с официальным списком горячих клавиш Figma.
pub const COMMANDS: &[Command] = &[
    cmd(C::ToolMove, "tool.move", "Перемещение", &["KeyV"]),
    cmd(C::ToolFrame, "tool.frame", "Фрейм", &["KeyF", "KeyA"]),
    cmd(C::ToolRect, "tool.rect", "Прямоугольник", &["KeyR"]),
    cmd(C::ToolEllipse, "tool.ellipse", "Эллипс", &["KeyO"]),
    cmd(C::ToolLine, "tool.line", "Линия", &["KeyL"]),
    cmd(C::ToolHand, "tool.hand", "Рука", &["KeyH"]),
    cmd(C::ToolText, "tool.text", "Текст", &["KeyT"]),
    cmd(C::ToolPen, "tool.pen", "Перо", &["KeyP"]),
    cmd(
        C::HandHold,
        "tool.hand_hold",
        "Рука, пока клавиша нажата",
        &["Space"],
    ),
    cmd(
        C::ToolImage,
        "tool.image",
        "Поместить изображение",
        &["Mod+Shift+KeyK"],
    ),
    cmd(C::Undo, "edit.undo", "Отменить", &["Mod+KeyZ"]),
    Command {
        id: C::Redo,
        key: "edit.redo",
        name: "Повторить",
        defaults: &["Mod+Shift+KeyZ"],
        defaults_pc: &["Ctrl+KeyY"],
        defaults_mac: &[],
    },
    cmd(C::Copy, "edit.copy", "Копировать", &["Mod+KeyC"]),
    cmd(C::Cut, "edit.cut", "Вырезать", &["Mod+KeyX"]),
    cmd(C::Paste, "edit.paste", "Вставить", &["Mod+KeyV"]),
    cmd(
        C::CopyAsPng,
        "edit.copy_png",
        "Копировать как PNG",
        &["Mod+Shift+KeyC"],
    ),
    cmd(C::Duplicate, "edit.duplicate", "Дублировать", &["Mod+KeyD"]),
    cmd(
        C::Delete,
        "edit.delete",
        "Удалить",
        &["Delete", "Backspace"],
    ),
    cmd(
        C::SelectAll,
        "edit.select_all",
        "Выделить всё",
        &["Mod+KeyA"],
    ),
    cmd(C::Escape, "edit.escape", "Отмена", &["Escape"]),
    cmd(C::Group, "object.group", "Сгруппировать", &["Mod+KeyG"]),
    cmd(
        C::Ungroup,
        "object.ungroup",
        "Разгруппировать",
        &["Mod+Shift+KeyG"],
    ),
    cmd(
        C::FrameSelection,
        "object.frame_selection",
        "Обернуть во фрейм",
        &["Mod+Alt+KeyG"],
    ),
    cmd(
        C::BringForward,
        "arrange.forward",
        "Переместить вперёд",
        &["Mod+BracketRight"],
    ),
    cmd(
        C::SendBackward,
        "arrange.backward",
        "Переместить назад",
        &["Mod+BracketLeft"],
    ),
    // На Windows и Linux Ctrl+Alt — это AltGr (символы на многих раскладках),
    // поэтому там, как в Figma для Windows, — Ctrl+Shift+] / Ctrl+Shift+[.
    cmd_per_platform(
        C::BringToFront,
        "arrange.front",
        "На передний план",
        &["Mod+Alt+BracketRight"],
        &["Ctrl+Shift+BracketRight"],
    ),
    cmd_per_platform(
        C::SendToBack,
        "arrange.back",
        "На задний план",
        &["Mod+Alt+BracketLeft"],
        &["Ctrl+Shift+BracketLeft"],
    ),
    cmd(C::NudgeLeft, "nudge.left", "Сдвинуть влево", &["ArrowLeft"]),
    cmd(
        C::NudgeRight,
        "nudge.right",
        "Сдвинуть вправо",
        &["ArrowRight"],
    ),
    cmd(C::NudgeUp, "nudge.up", "Сдвинуть вверх", &["ArrowUp"]),
    cmd(C::NudgeDown, "nudge.down", "Сдвинуть вниз", &["ArrowDown"]),
    cmd(
        C::NudgeLeftBig,
        "nudge.left_big",
        "Сдвинуть влево на 10 px",
        &["Shift+ArrowLeft"],
    ),
    cmd(
        C::NudgeRightBig,
        "nudge.right_big",
        "Сдвинуть вправо на 10 px",
        &["Shift+ArrowRight"],
    ),
    cmd(
        C::NudgeUpBig,
        "nudge.up_big",
        "Сдвинуть вверх на 10 px",
        &["Shift+ArrowUp"],
    ),
    cmd(
        C::NudgeDownBig,
        "nudge.down_big",
        "Сдвинуть вниз на 10 px",
        &["Shift+ArrowDown"],
    ),
    cmd(
        C::ZoomToFit,
        "view.fit_all",
        "Показать всё",
        &["Shift+Digit1"],
    ),
    cmd(
        C::ZoomToSelection,
        "view.fit_selection",
        "Показать выделение",
        &["Shift+Digit2"],
    ),
    cmd(
        C::Zoom100,
        "view.zoom_100",
        "Масштаб 100%",
        &["Shift+Digit0"],
    ),
    // Shift+= — это «+» на раскладке US; цифровой блок — как в Figma.
    cmd(
        C::ZoomIn,
        "view.zoom_in",
        "Приблизить",
        &["Equal", "Shift+Equal", "NumpadAdd"],
    ),
    cmd(
        C::ZoomOut,
        "view.zoom_out",
        "Отдалить",
        &["Minus", "NumpadSubtract"],
    ),
    cmd(
        C::ToggleVisible,
        "object.toggle_visible",
        "Скрыть или показать",
        &["Mod+Shift+KeyH"],
    ),
    cmd(
        C::ToggleLock,
        "object.toggle_lock",
        "Заблокировать или разблокировать",
        &["Mod+Shift+KeyL"],
    ),
    cmd(
        C::ToggleUi,
        "view.toggle_ui",
        "Скрыть интерфейс",
        &["Mod+Backslash"],
    ),
    // Shift+H и Shift+V, как в Figma: H без модификаторов — «Рука», V — «Перемещение»,
    // Ctrl/Cmd+Shift+H — «Скрыть»; сочетания сравниваются точным набором модификаторов.
    cmd(
        C::FlipHorizontal,
        "object.flip_horizontal",
        "Отразить по горизонтали",
        &["Shift+KeyH"],
    ),
    cmd(
        C::FlipVertical,
        "object.flip_vertical",
        "Отразить по вертикали",
        &["Shift+KeyV"],
    ),
    cmd(C::Save, "file.save", "Сохранить", &["Mod+KeyS"]),
];

impl CommandId {
    pub fn info(self) -> &'static Command {
        COMMANDS
            .iter()
            .find(|c| c.id == self)
            .expect("каждая команда есть в таблице")
    }

    pub fn key(self) -> &'static str {
        self.info().key
    }

    pub fn name(self) -> &'static str {
        self.info().name
    }

    pub fn from_key(key: &str) -> Option<CommandId> {
        COMMANDS.iter().find(|c| c.key == key).map(|c| c.id)
    }

    /// Сочетания по умолчанию для платформы, как строки с явными модификаторами.
    pub fn default_shortcuts(self, platform: Platform) -> Vec<String> {
        let info = self.info();
        let primary = match platform {
            Platform::Mac => "Meta",
            _ => "Ctrl",
        };
        let pc: &[&str] = if platform == Platform::Mac {
            info.defaults_mac
        } else {
            info.defaults_pc
        };
        info.defaults
            .iter()
            .chain(pc)
            .map(|s| s.replace("Mod", primary))
            .collect()
    }
}
