//! Единая таблица команд: меню интерфейса, палитра команд и горячие клавиши вызывают
//! одно и то же по идентификатору. Сочетания по умолчанию повторяют Figma: на macOS
//! главная клавиша — Cmd, на Windows и Linux — Ctrl (в таблице это `Mod`).
//!
//! У каждой команды — группа (подменю главного меню), синонимы для поиска и
//! признак переключателя (спека подсистемы 2, раздел 3). Команды `ui.*`
//! выполняет интерфейс: ядро только сообщает о вызове.

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
    /// «Стрелка»: линия со стрелкой на конце.
    ToolArrow,
    /// «Масштаб» (K, подсистема 3, этап 5).
    ToolScale,
    /// «Сплющить»: выделение — в один вектор.
    Flatten,
    /// Булевы операции над выделением (подсистема 3, этап 4); у выделенных
    /// булевых групп — смена операции.
    BooleanUnion,
    BooleanSubtract,
    BooleanIntersect,
    BooleanExclude,
    /// «Контур обводки»: обводки выделения — в векторы с заливкой их цветом.
    OutlineStroke,
    /// Рука, пока клавиша удерживается (по умолчанию Space).
    HandHold,
    ToolImage,
    Undo,
    Redo,
    Copy,
    Cut,
    Paste,
    CopyAsPng,
    /// «Копировать как SVG» (подсистема 3, этап 5): без сочетания, как в Figma.
    CopyAsSvg,
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
    /// Пиксельная сетка при зуме от 800% (Shift+', как в Figma).
    PixelGrid,
    /// Войти в правку выделенного: текст — правка текста, вектор и фигура — правка
    /// вектора (как Enter).
    EditObject,
    /// Стиль текста (в правке — выделенного фрагмента, иначе — всего текста
    /// выделенных узлов), как Ctrl/Cmd+B, I, U, Shift+X и выравнивание.
    TextBold,
    TextItalic,
    TextUnderline,
    TextStrike,
    TextAlignLeft,
    TextAlignCenter,
    TextAlignRight,
    TextAlignJustify,
    // ——— Команды интерфейса (`ui.*`): их выполняет основной поток, ядро только
    // сообщает о вызове событием `EditorEvent::UiCommand` (спека подсистемы 2,
    // раздел 3). ———
    /// Открыть главное меню (Alt+/).
    UiMainMenu,
    UiNewFile,
    UiOpen,
    /// «Открыть с диска…» (File System Access и десктоп).
    UiOpenDisk,
    UiSaveDisk,
    UiSaveCopy,
    /// «Экспорт…»: секция экспорта панели свойств.
    UiExport,
    UiAllFiles,
    UiSettings,
    UiShortcuts,
    UiAbout,
    /// Привязка к пикселям (настройка пользователя).
    UiPixelSnap,
    UiThemeDark,
    UiThemeLight,
    UiThemeSystem,
}

/// Группа команды: подменю главного меню и раздел таблицы сочетаний.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum CommandGroup {
    Tool,
    File,
    Edit,
    View,
    Object,
    Text,
    Arrange,
    Vector,
    Nudge,
    Settings,
    Help,
}

impl CommandGroup {
    pub const ALL: [CommandGroup; 11] = [
        CommandGroup::Tool,
        CommandGroup::File,
        CommandGroup::Edit,
        CommandGroup::View,
        CommandGroup::Object,
        CommandGroup::Text,
        CommandGroup::Arrange,
        CommandGroup::Vector,
        CommandGroup::Nudge,
        CommandGroup::Settings,
        CommandGroup::Help,
    ];

    /// Ключ группы в протоколе (`KeymapEntry.group`).
    pub fn key(self) -> &'static str {
        match self {
            CommandGroup::Tool => "tool",
            CommandGroup::File => "file",
            CommandGroup::Edit => "edit",
            CommandGroup::View => "view",
            CommandGroup::Object => "object",
            CommandGroup::Text => "text",
            CommandGroup::Arrange => "arrange",
            CommandGroup::Vector => "vector",
            CommandGroup::Nudge => "nudge",
            CommandGroup::Settings => "settings",
            CommandGroup::Help => "help",
        }
    }

    /// Название подменю и раздела.
    pub fn title(self) -> &'static str {
        match self {
            CommandGroup::Tool => "Инструменты",
            CommandGroup::File => "Файл",
            CommandGroup::Edit => "Правка",
            CommandGroup::View => "Вид",
            CommandGroup::Object => "Объект",
            CommandGroup::Text => "Текст",
            CommandGroup::Arrange => "Расставить",
            CommandGroup::Vector => "Вектор",
            CommandGroup::Nudge => "Сдвиг",
            CommandGroup::Settings => "Настройки",
            CommandGroup::Help => "Справка",
        }
    }
}

/// Строка таблицы команд.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Command {
    pub id: CommandId,
    /// Стабильный ключ для настроек, меню и JSON раскладки.
    pub key: &'static str,
    /// Группа: подменю главного меню и раздел таблицы сочетаний.
    pub group: CommandGroup,
    /// Название в интерфейсе.
    pub name: &'static str,
    /// Синонимы для поиска по командам — русские и английские («дубль, duplicate»).
    pub keywords: &'static [&'static str],
    /// Переключатель: у пункта меню галочка (`checked` в состоянии команды).
    pub toggle: bool,
    /// Сочетания по умолчанию: `Mod` — Cmd на macOS, Ctrl на остальных.
    pub defaults: &'static [&'static str],
    /// Дополнительные сочетания только для Windows и Linux.
    pub defaults_pc: &'static [&'static str],
    /// Дополнительные сочетания только для macOS.
    pub defaults_mac: &'static [&'static str],
}

/// Строка таблицы. Группа и синонимы обязательны: без них таблица не собирается.
const fn cmd(
    id: CommandId,
    key: &'static str,
    group: CommandGroup,
    name: &'static str,
    keywords: &'static [&'static str],
    defaults: &'static [&'static str],
) -> Command {
    Command {
        id,
        key,
        group,
        name,
        keywords,
        toggle: false,
        defaults,
        defaults_pc: &[],
        defaults_mac: &[],
    }
}

impl Command {
    /// Переключатель (галочка в меню).
    const fn toggle(self) -> Command {
        Command {
            toggle: true,
            ..self
        }
    }

    /// Дополнительные сочетания только для Windows и Linux.
    const fn pc(self, pc: &'static [&'static str]) -> Command {
        Command {
            defaults_pc: pc,
            ..self
        }
    }

    /// Разные сочетания на macOS и на Windows/Linux (вместо общих).
    const fn per_platform(
        self,
        mac: &'static [&'static str],
        pc: &'static [&'static str],
    ) -> Command {
        Command {
            defaults: &[],
            defaults_mac: mac,
            defaults_pc: pc,
            ..self
        }
    }
}

use CommandGroup as G;
use CommandId as C;

/// Все команды. Сочетания сверены с официальным списком горячих клавиш Figma.
pub const COMMANDS: &[Command] = &[
    cmd(
        C::ToolMove,
        "tool.move",
        G::Tool,
        "Перемещение",
        &["выбор", "курсор", "стрелка", "move", "select"],
        &["KeyV"],
    ),
    cmd(
        C::ToolFrame,
        "tool.frame",
        G::Tool,
        "Фрейм",
        &["монтажная область", "артборд", "frame", "artboard"],
        &["KeyF", "KeyA"],
    ),
    cmd(
        C::ToolRect,
        "tool.rect",
        G::Tool,
        "Прямоугольник",
        &["квадрат", "фигура", "rectangle", "square"],
        &["KeyR"],
    ),
    cmd(
        C::ToolEllipse,
        "tool.ellipse",
        G::Tool,
        "Эллипс",
        &["круг", "овал", "ellipse", "circle"],
        &["KeyO"],
    ),
    cmd(
        C::ToolLine,
        "tool.line",
        G::Tool,
        "Линия",
        &["отрезок", "line"],
        &["KeyL"],
    ),
    cmd(
        C::ToolHand,
        "tool.hand",
        G::Tool,
        "Рука",
        &["панорама", "прокрутка", "hand", "pan"],
        &["KeyH"],
    ),
    cmd(
        C::ToolText,
        "tool.text",
        G::Tool,
        "Текст",
        &["надпись", "шрифт", "text", "type"],
        &["KeyT"],
    ),
    cmd(
        C::ToolPen,
        "tool.pen",
        G::Tool,
        "Перо",
        &["кривая", "путь", "pen", "path", "bezier"],
        &["KeyP"],
    ),
    cmd(
        C::ToolScale,
        "tool.scale",
        G::Tool,
        "Масштабирование",
        &["пропорционально", "размер", "scale", "resize"],
        &["KeyK"],
    ),
    cmd(
        C::ToolArrow,
        "tool.arrow",
        G::Tool,
        "Стрелка",
        &["указатель", "линия", "arrow"],
        &["Shift+KeyL"],
    ),
    cmd(
        C::HandHold,
        "tool.hand_hold",
        G::Tool,
        "Рука, пока клавиша нажата",
        &["пробел", "панорама", "hand", "space", "pan"],
        &["Space"],
    ),
    cmd(
        C::ToolImage,
        "tool.image",
        G::Tool,
        "Поместить изображение",
        &["картинка", "фото", "image", "place", "picture"],
        &["Mod+Shift+KeyK"],
    ),
    cmd(
        C::Undo,
        "edit.undo",
        G::Edit,
        "Отменить",
        &["назад", "откат", "undo"],
        &["Mod+KeyZ"],
    ),
    cmd(
        C::Redo,
        "edit.redo",
        G::Edit,
        "Повторить",
        &["вернуть", "redo"],
        &["Mod+Shift+KeyZ"],
    )
    .pc(&["Ctrl+KeyY"]),
    cmd(
        C::Copy,
        "edit.copy",
        G::Edit,
        "Копировать",
        &["буфер", "copy"],
        &["Mod+KeyC"],
    ),
    cmd(
        C::Cut,
        "edit.cut",
        G::Edit,
        "Вырезать",
        &["буфер", "cut"],
        &["Mod+KeyX"],
    ),
    cmd(
        C::Paste,
        "edit.paste",
        G::Edit,
        "Вставить",
        &["буфер", "paste"],
        &["Mod+KeyV"],
    ),
    cmd(
        C::CopyAsPng,
        "edit.copy_png",
        G::Edit,
        "Копировать как PNG",
        &["картинка", "растр", "copy as png", "image"],
        &["Mod+Shift+KeyC"],
    ),
    cmd(
        C::CopyAsSvg,
        "edit.copy_svg",
        G::Edit,
        "Копировать как SVG",
        &["вектор", "код", "copy as svg", "vector"],
        &[],
    ),
    cmd(
        C::Duplicate,
        "edit.duplicate",
        G::Edit,
        "Дублировать",
        &["дубль", "копия", "duplicate", "clone"],
        &["Mod+KeyD"],
    ),
    cmd(
        C::Delete,
        "edit.delete",
        G::Edit,
        "Удалить",
        &["стереть", "убрать", "delete", "remove"],
        &["Delete", "Backspace"],
    ),
    cmd(
        C::SelectAll,
        "edit.select_all",
        G::Edit,
        "Выделить всё",
        &["все слои", "select all"],
        &["Mod+KeyA"],
    ),
    cmd(
        C::Escape,
        "edit.escape",
        G::Edit,
        "Отмена",
        &["снять выделение", "выход", "escape", "deselect"],
        &["Escape"],
    ),
    cmd(
        C::Group,
        "object.group",
        G::Object,
        "Сгруппировать",
        &["группа", "объединить", "group"],
        &["Mod+KeyG"],
    ),
    cmd(
        C::Ungroup,
        "object.ungroup",
        G::Object,
        "Разгруппировать",
        &["разобрать", "ungroup"],
        &["Mod+Shift+KeyG"],
    ),
    cmd(
        C::Flatten,
        "object.flatten",
        G::Object,
        "Сплющить",
        &["в вектор", "контуры", "flatten"],
        &["Mod+KeyE"],
    ),
    // Как в Figma: Alt+Shift+буква (Option+Shift на macOS), исключение — E.
    // Свободны на обеих платформах: Alt+Shift и Shift+E в таблице ничем не заняты.
    cmd(
        C::BooleanUnion,
        "object.boolean_union",
        G::Object,
        "Объединение",
        &["булева", "сложить", "union", "boolean"],
        &["Alt+Shift+KeyU"],
    )
    .toggle(),
    cmd(
        C::BooleanSubtract,
        "object.boolean_subtract",
        G::Object,
        "Вычитание",
        &["булева", "вычесть", "subtract", "boolean"],
        &["Alt+Shift+KeyS"],
    )
    .toggle(),
    cmd(
        C::BooleanIntersect,
        "object.boolean_intersect",
        G::Object,
        "Пересечение",
        &["булева", "общая часть", "intersect", "boolean"],
        &["Alt+Shift+KeyI"],
    )
    .toggle(),
    cmd(
        C::BooleanExclude,
        "object.boolean_exclude",
        G::Object,
        "Исключение",
        &["булева", "xor", "exclude", "boolean"],
        &["Alt+Shift+KeyE"],
    )
    .toggle(),
    cmd(
        C::OutlineStroke,
        "object.outline_stroke",
        G::Object,
        "Контур обводки",
        &["обводка в заливку", "outline stroke", "stroke"],
        &["Mod+Alt+KeyO"],
    ),
    cmd(
        C::FrameSelection,
        "object.frame_selection",
        G::Object,
        "Обернуть во фрейм",
        &["фрейм", "контейнер", "frame selection", "wrap"],
        &["Mod+Alt+KeyG"],
    ),
    cmd(
        C::BringForward,
        "arrange.forward",
        G::Arrange,
        "Переместить вперёд",
        &["выше", "порядок", "bring forward", "order"],
        &["Mod+BracketRight"],
    ),
    cmd(
        C::SendBackward,
        "arrange.backward",
        G::Arrange,
        "Переместить назад",
        &["ниже", "порядок", "send backward", "order"],
        &["Mod+BracketLeft"],
    ),
    // На Windows и Linux Ctrl+Alt — это AltGr (символы на многих раскладках),
    // поэтому там, как в Figma для Windows, — Ctrl+Shift+] / Ctrl+Shift+[.
    cmd(
        C::BringToFront,
        "arrange.front",
        G::Arrange,
        "На передний план",
        &["наверх", "порядок", "bring to front", "order"],
        &[],
    )
    .per_platform(&["Mod+Alt+BracketRight"], &["Ctrl+Shift+BracketRight"]),
    cmd(
        C::SendToBack,
        "arrange.back",
        G::Arrange,
        "На задний план",
        &["вниз", "порядок", "send to back", "order"],
        &[],
    )
    .per_platform(&["Mod+Alt+BracketLeft"], &["Ctrl+Shift+BracketLeft"]),
    cmd(
        C::NudgeLeft,
        "nudge.left",
        G::Nudge,
        "Сдвинуть влево",
        &["сдвиг", "nudge", "left"],
        &["ArrowLeft"],
    ),
    cmd(
        C::NudgeRight,
        "nudge.right",
        G::Nudge,
        "Сдвинуть вправо",
        &["сдвиг", "nudge", "right"],
        &["ArrowRight"],
    ),
    cmd(
        C::NudgeUp,
        "nudge.up",
        G::Nudge,
        "Сдвинуть вверх",
        &["сдвиг", "nudge", "up"],
        &["ArrowUp"],
    ),
    cmd(
        C::NudgeDown,
        "nudge.down",
        G::Nudge,
        "Сдвинуть вниз",
        &["сдвиг", "nudge", "down"],
        &["ArrowDown"],
    ),
    cmd(
        C::NudgeLeftBig,
        "nudge.left_big",
        G::Nudge,
        "Сдвинуть влево на 10 px",
        &["сдвиг", "nudge", "left"],
        &["Shift+ArrowLeft"],
    ),
    cmd(
        C::NudgeRightBig,
        "nudge.right_big",
        G::Nudge,
        "Сдвинуть вправо на 10 px",
        &["сдвиг", "nudge", "right"],
        &["Shift+ArrowRight"],
    ),
    cmd(
        C::NudgeUpBig,
        "nudge.up_big",
        G::Nudge,
        "Сдвинуть вверх на 10 px",
        &["сдвиг", "nudge", "up"],
        &["Shift+ArrowUp"],
    ),
    cmd(
        C::NudgeDownBig,
        "nudge.down_big",
        G::Nudge,
        "Сдвинуть вниз на 10 px",
        &["сдвиг", "nudge", "down"],
        &["Shift+ArrowDown"],
    ),
    cmd(
        C::ZoomToFit,
        "view.fit_all",
        G::View,
        "Показать всё",
        &["масштаб", "вписать", "zoom to fit", "fit"],
        &["Shift+Digit1"],
    ),
    cmd(
        C::ZoomToSelection,
        "view.fit_selection",
        G::View,
        "Показать выделение",
        &["масштаб", "к выделению", "zoom to selection"],
        &["Shift+Digit2"],
    ),
    cmd(
        C::Zoom100,
        "view.zoom_100",
        G::View,
        "Масштаб 100%",
        &["реальный размер", "zoom 100", "actual size"],
        &["Shift+Digit0"],
    ),
    // Shift+= — это «+» на раскладке US; цифровой блок — как в Figma.
    cmd(
        C::ZoomIn,
        "view.zoom_in",
        G::View,
        "Приблизить",
        &["масштаб", "увеличить", "zoom in"],
        &["Equal", "Shift+Equal", "NumpadAdd"],
    ),
    cmd(
        C::ZoomOut,
        "view.zoom_out",
        G::View,
        "Отдалить",
        &["масштаб", "уменьшить", "zoom out"],
        &["Minus", "NumpadSubtract"],
    ),
    cmd(
        C::ToggleVisible,
        "object.toggle_visible",
        G::Object,
        "Скрыть или показать",
        &["видимость", "спрятать", "hide", "show", "visibility"],
        &["Mod+Shift+KeyH"],
    ),
    cmd(
        C::ToggleLock,
        "object.toggle_lock",
        G::Object,
        "Заблокировать или разблокировать",
        &["замок", "закрепить", "lock", "unlock"],
        &["Mod+Shift+KeyL"],
    ),
    cmd(
        C::ToggleUi,
        "view.toggle_ui",
        G::View,
        "Скрыть интерфейс",
        &["панели", "полный экран", "hide ui", "interface"],
        &["Mod+Backslash"],
    )
    .toggle(),
    // Shift+H и Shift+V, как в Figma: H без модификаторов — «Рука», V — «Перемещение»,
    // Ctrl/Cmd+Shift+H — «Скрыть»; сочетания сравниваются точным набором модификаторов.
    cmd(
        C::FlipHorizontal,
        "object.flip_horizontal",
        G::Object,
        "Отразить по горизонтали",
        &["зеркало", "перевернуть", "flip horizontal", "mirror"],
        &["Shift+KeyH"],
    ),
    cmd(
        C::FlipVertical,
        "object.flip_vertical",
        G::Object,
        "Отразить по вертикали",
        &["зеркало", "перевернуть", "flip vertical", "mirror"],
        &["Shift+KeyV"],
    ),
    cmd(
        C::Save,
        "file.save",
        G::File,
        "Сохранить",
        &["запись", "save"],
        &["Mod+KeyS"],
    ),
    // Как в Figma: «Pixel grid» — Shift+'.
    cmd(
        C::PixelGrid,
        "view.pixel_grid",
        G::View,
        "Пиксельная сетка",
        &["сетка", "пиксели", "pixel grid", "grid"],
        &["Shift+Quote"],
    )
    .toggle(),
    // Enter — физическая клавиша правки (как Esc у жеста); в таблице — для меню
    // и подписи.
    cmd(
        C::EditObject,
        "vector.edit",
        G::Vector,
        "Войти в правку",
        &["редактировать", "точки", "узлы", "edit object", "edit"],
        &["Enter"],
    ),
    // Форматирование текста — как в справке Figma; в правке текста действует
    // та же раскладка.
    cmd(
        C::TextBold,
        "text.bold",
        G::Text,
        "Жирный",
        &["полужирный", "толщина", "bold"],
        &["Mod+KeyB"],
    ),
    cmd(
        C::TextItalic,
        "text.italic",
        G::Text,
        "Курсив",
        &["наклонный", "italic"],
        &["Mod+KeyI"],
    ),
    cmd(
        C::TextUnderline,
        "text.underline",
        G::Text,
        "Подчёркнутый",
        &["подчеркнуть", "underline"],
        &["Mod+KeyU"],
    ),
    cmd(
        C::TextStrike,
        "text.strike",
        G::Text,
        "Зачёркнутый",
        &["зачеркнуть", "strikethrough"],
        &["Mod+Shift+KeyX"],
    ),
    cmd(
        C::TextAlignLeft,
        "text.align_left",
        G::Text,
        "Выровнять по левому краю",
        &["выравнивание", "влево", "align left"],
        &["Mod+Alt+KeyL"],
    ),
    cmd(
        C::TextAlignCenter,
        "text.align_center",
        G::Text,
        "Выровнять по центру",
        &["выравнивание", "центр", "align center"],
        &["Mod+Alt+KeyT"],
    ),
    cmd(
        C::TextAlignRight,
        "text.align_right",
        G::Text,
        "Выровнять по правому краю",
        &["выравнивание", "вправо", "align right"],
        &["Mod+Alt+KeyR"],
    ),
    cmd(
        C::TextAlignJustify,
        "text.align_justify",
        G::Text,
        "Выровнять по ширине",
        &["выравнивание", "по ширине", "justify"],
        &["Mod+Alt+KeyJ"],
    ),
    // Команды интерфейса. Ctrl/Cmd+N и Ctrl/Cmd+O у браузера (новое окно, открыть
    // файл страницей), поэтому у «Нового файла» и «Открыть…» сочетаний нет.
    cmd(
        C::UiMainMenu,
        "ui.main_menu",
        G::View,
        "Главное меню",
        &["меню", "поиск команд", "main menu", "menu"],
        &["Alt+Slash"],
    ),
    cmd(
        C::UiNewFile,
        "ui.new_file",
        G::File,
        "Новый файл",
        &["создать", "новый документ", "new file", "create"],
        &[],
    ),
    cmd(
        C::UiOpen,
        "ui.open",
        G::File,
        "Открыть…",
        &["импорт", "загрузить", "open", "import"],
        &[],
    ),
    cmd(
        C::UiOpenDisk,
        "ui.open_disk",
        G::File,
        "Открыть с диска…",
        &["файл на диске", "open from disk", "local file"],
        &[],
    ),
    cmd(
        C::UiSaveDisk,
        "ui.save_disk",
        G::File,
        "Сохранить на диск…",
        &["файл на диске", "save to disk", "save as"],
        &[],
    ),
    cmd(
        C::UiSaveCopy,
        "ui.save_copy",
        G::File,
        "Сохранить копию на диск…",
        &["скачать", "копия", "save copy", "download"],
        &[],
    ),
    // Ctrl/Cmd+Shift+E — «Экспорт…» в Figma.
    cmd(
        C::UiExport,
        "ui.export",
        G::File,
        "Экспорт…",
        &["выгрузить", "png", "svg", "pdf", "export"],
        &["Mod+Shift+KeyE"],
    ),
    cmd(
        C::UiAllFiles,
        "ui.all_files",
        G::File,
        "Все файлы",
        &[
            "назад к файлам",
            "список файлов",
            "all files",
            "back to files",
        ],
        &[],
    ),
    cmd(
        C::UiSettings,
        "ui.settings",
        G::Settings,
        "Настройки…",
        &["параметры", "preferences", "settings"],
        &[],
    ),
    cmd(
        C::UiShortcuts,
        "ui.shortcuts",
        G::Help,
        "Сочетания клавиш",
        &[
            "горячие клавиши",
            "раскладка",
            "keyboard shortcuts",
            "hotkeys",
        ],
        &[],
    ),
    cmd(
        C::UiAbout,
        "ui.about",
        G::Help,
        "О программе",
        &["версия", "справка", "about", "version"],
        &[],
    ),
    // Как в Figma: «Snap to pixel grid» — Ctrl/Cmd+Shift+'.
    cmd(
        C::UiPixelSnap,
        "ui.pixel_snap",
        G::Settings,
        "Привязка к пикселям",
        &[
            "пиксельная привязка",
            "целые пиксели",
            "snap to pixel grid",
            "pixel snap",
        ],
        &["Mod+Shift+Quote"],
    )
    .toggle(),
    cmd(
        C::UiThemeDark,
        "ui.theme_dark",
        G::Settings,
        "Тёмная тема",
        &["тема", "оформление", "dark theme", "theme"],
        &[],
    )
    .toggle(),
    cmd(
        C::UiThemeLight,
        "ui.theme_light",
        G::Settings,
        "Светлая тема",
        &["тема", "оформление", "light theme", "theme"],
        &[],
    )
    .toggle(),
    cmd(
        C::UiThemeSystem,
        "ui.theme_system",
        G::Settings,
        "Тема как в системе",
        &["тема", "оформление", "system theme", "theme"],
        &[],
    )
    .toggle(),
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

    pub fn group(self) -> CommandGroup {
        self.info().group
    }

    /// Команда интерфейса (`ui.*`): её выполняет основной поток по событию
    /// `EditorEvent::UiCommand`.
    pub fn is_ui(self) -> bool {
        self.key().starts_with("ui.")
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
