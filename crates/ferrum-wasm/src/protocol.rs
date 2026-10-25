//! Сообщения между основным потоком и worker'ом. Типы описаны здесь, TypeScript-
//! типы генерирует `tsify` (они попадают в `ferrum_wasm.d.ts` при сборке
//! wasm-bindgen). Через `postMessage` они идут как обычные объекты
//! (serde-wasm-bindgen), байты — `Uint8Array`, буфер которых передаётся как
//! transferable.
//!
//! Команды от основного потока не доверенные: неизвестная команда, лишние или
//! неверные поля дают событие `Error`, а не панику worker'а.

use serde::{Deserialize, Serialize};
use serde_bytes::ByteBuf;
use tsify::Tsify;

pub use crate::layers::{LayerRow, NodeType};
pub use crate::summary::{
    Align, FillDto, Flag, Num, PaintKind, Paints, ScaleModeDto, StrokeDto, Summary,
};

/// Наибольшая страница-пример: столько узлов генерирует `Demo`.
pub const MAX_DEMO_NODES: u32 = 500_000;
/// Сколько идентификаторов выделения уходит в событии `Selection`: панели
/// получают из worker'а готовые срезы, полный список в основной поток не нужен.
pub const SELECTION_IDS_MAX: usize = 1000;
/// Предел байтов строки в команде (имя, запрос поиска, сочетание): имена режутся
/// до 256 символов, но строка из основного потока может быть любой длины.
pub const MAX_TEXT_BYTES: usize = 4096;
/// Масштабы экспорта PNG (раздел 8.7 спеки).
pub const EXPORT_SCALES: [f64; 3] = [1.0, 2.0, 3.0];

/// Инструмент.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[serde(rename_all = "camelCase")]
pub enum Tool {
    Move,
    Frame,
    Rect,
    Ellipse,
    Line,
    Hand,
    Image,
}

/// Тема оверлеев.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[serde(rename_all = "camelCase")]
pub enum Theme {
    Dark,
    Light,
}

/// CSS-курсор над холстом. У ресайза и поворота — угол на экране в градусах.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[serde(rename_all = "camelCase")]
pub enum CursorKind {
    Default,
    Crosshair,
    Grab,
    Grabbing,
    Duplicate,
    Resize,
    Rotate,
}

/// Бэкенд рендера (`layout::BACKENDS`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[serde(rename_all = "camelCase")]
pub enum Backend {
    Webgpu,
    Webgl,
    Cpu,
}

impl Backend {
    /// Индекс в `layout::BACKENDS` (живое состояние).
    pub fn code(self) -> f64 {
        match self {
            Backend::Webgpu => 1.0,
            Backend::Webgl => 2.0,
            Backend::Cpu => 3.0,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Backend::Webgpu => "webgpu",
            Backend::Webgl => "webgl",
            Backend::Cpu => "cpu",
        }
    }

    pub fn parse(s: &str) -> Option<Backend> {
        match s {
            "webgpu" => Some(Backend::Webgpu),
            "webgl" => Some(Backend::Webgl),
            "cpu" => Some(Backend::Cpu),
            _ => None,
        }
    }
}

/// Как выделять из панели слоёв: заменить, добавить или убрать (Ctrl/Cmd),
/// диапазон от прошлого щелчка (Shift).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[serde(rename_all = "camelCase")]
pub enum SelectMode {
    Replace,
    Toggle,
    Range,
}

/// Куда бросили слои относительно строки.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[serde(rename_all = "camelCase")]
pub enum DropPosition {
    Above,
    Below,
    Into,
}

/// Числовое поле панели свойств.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[serde(rename_all = "camelCase")]
pub enum Field {
    X,
    Y,
    W,
    H,
    Rotation,
    Opacity,
    CornerRadius,
    StrokeWidth,
}

/// Фаза правки из панели: превью жеста, запись одним шагом, отмена жеста.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[serde(rename_all = "camelCase")]
pub enum EditPhase {
    Preview,
    Commit,
    Cancel,
}

/// Правка выделения из панели свойств.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Tsify)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum Edit {
    Number {
        field: Field,
        value: f64,
        relative: bool,
    },
    Fills {
        fills: Vec<FillDto>,
    },
    Strokes {
        strokes: Vec<StrokeDto>,
    },
    ClipContent {
        value: bool,
    },
    Size {
        w: f64,
        h: f64,
    },
}

/// Команда основного потока worker'у.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Tsify)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum Command {
    /// Открыть файл `.ferrum`.
    Open {
        #[tsify(type = "Uint8Array")]
        bytes: ByteBuf,
    },
    /// Новый документ — страница-пример из `nodes` узлов (фреймы по 100 узлов).
    Demo {
        nodes: u32,
    },
    /// Команда редактора по ключу таблицы команд (`edit.undo`, `tool.rect`…).
    Execute {
        command: String,
    },
    SetTool {
        tool: Tool,
    },
    SetTheme {
        theme: Theme,
    },
    /// Вставить содержимое буфера обмена (формат `web application/x-ferrum`).
    Paste {
        #[tsify(type = "Uint8Array")]
        bytes: ByteBuf,
    },
    /// Разместить изображение (PNG): следующий клик ставит его на холст.
    PlaceImage {
        #[tsify(type = "Uint8Array")]
        bytes: ByteBuf,
    },
    /// Сохранить документ: ответ — событие `Saved`.
    Save,
    /// Прочитать пиксели текущего кадра (для тестов): ответ — `Pixels`.
    /// `reference` — тот же кадр, нарисованный `vello_cpu`, для сравнения с GPU.
    ReadPixels {
        reference: bool,
    },
    // ——— Этап 5: панели ———
    /// Окно строк панели слоёв, которое сейчас видно (с запасом).
    LayersWindow {
        start: u32,
        count: u32,
    },
    ExpandLayer {
        id: String,
        expanded: bool,
    },
    /// Поиск по слоям; пустая строка — без поиска.
    FilterLayers {
        query: String,
    },
    /// Построить кэш имён для поиска заранее (поле поиска получило фокус).
    PrepareSearch,
    /// Выделение из панели слоёв (не больше `SELECTION_IDS_MAX` идентификаторов).
    Select {
        ids: Vec<String>,
        mode: SelectMode,
    },
    /// Наведение на строку слоя — подсветка узла на холсте.
    HoverLayer {
        id: Option<String>,
    },
    RenameLayer {
        id: String,
        name: String,
    },
    /// Перенос слоёв к строке `target`. Без `ids` — текущее выделение целиком
    /// (оно может быть больше того, что видит панель).
    MoveLayers {
        ids: Option<Vec<String>>,
        target: String,
        position: DropPosition,
    },
    /// Скрыть или заблокировать (кнопки строки слоя).
    SetLayerFlags {
        ids: Vec<String>,
        visible: Option<bool>,
        locked: Option<bool>,
    },
    /// Правка выделения из панели свойств.
    EditSelection {
        edit: Edit,
        phase: EditPhase,
    },
    /// Новая страница; она становится текущей.
    AddPage,
    RenamePage {
        id: String,
        name: String,
    },
    DeletePage {
        id: String,
    },
    SetPage {
        id: String,
    },
    /// Фон текущей страницы (`#rrggbb[aa]`); без цвета — как у холста в теме.
    /// Жест палитры — превью только в кадре (`preview`), в документ — при
    /// отпускании (`commit`), `cancel` возвращает фон документа.
    SetPageBackground {
        color: Option<String>,
        phase: EditPhase,
    },
    SetDocumentName {
        name: String,
    },
    /// Фрейм из пресета (номер в списке события `Presets`).
    AddFramePreset {
        index: u32,
    },
    /// Зум к центру холста (1 — 100 %).
    SetZoom {
        zoom: f64,
    },
    /// Экспорт PNG выделения (без выделения — всей страницы): ответ — `Exported`.
    Export {
        scale: f64,
    },
    /// Изображение из системного буфера: встаёт в центр видимой части.
    PasteImage {
        #[tsify(type = "Uint8Array")]
        bytes: ByteBuf,
    },
    /// Назначить сочетание. Занято другими и `replace: false` — ответ `BindConflict`.
    Bind {
        command: String,
        shortcut: String,
        replace: bool,
    },
    Unbind {
        command: String,
        shortcut: String,
    },
    /// Сбросить сочетания команды; без команды — всех.
    ResetKeymap {
        command: Option<String>,
    },
    /// Отличия раскладки в JSON (`ferrum-keymap`) — из настроек или из файла.
    ImportKeymap {
        json: String,
    },
    SetSettings {
        pixel_snap: bool,
    },
}

/// Страница в списке.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Tsify)]
#[serde(rename_all = "camelCase")]
pub struct PageInfo {
    pub id: String,
    pub name: String,
}

/// Команда в таблице раскладки: ключ, название, группа (`tool`, `edit`…),
/// действующие сочетания (каноническая запись и подпись US) и умолчания.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Tsify)]
#[serde(rename_all = "camelCase")]
pub struct KeymapEntry {
    pub key: String,
    pub name: String,
    pub group: String,
    pub bindings: Vec<String>,
    pub labels: Vec<String>,
    pub defaults: Vec<String>,
    pub overridden: bool,
}

/// Сочетание, занятое несколькими командами (ключи команд).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Tsify)]
#[serde(rename_all = "camelCase")]
pub struct ConflictInfo {
    pub shortcut: String,
    pub label: String,
    pub commands: Vec<String>,
}

/// Пресет фрейма.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Tsify)]
#[serde(rename_all = "camelCase")]
pub struct PresetInfo {
    pub group: String,
    pub name: String,
    pub width: f64,
    pub height: f64,
}

/// Событие worker'а основному потоку.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Tsify)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum CoreEvent {
    /// Ядро запущено: бэкенд рендера и почему не подошли более быстрые.
    Ready {
        backend: Backend,
        fallbacks: Vec<String>,
    },
    /// Документ открыт; предупреждения — о частях повреждённого файла.
    Opened {
        pages: u32,
        nodes: u32,
        warnings: Vec<String>,
        /// Имя документа (для шапки и «Сохранить как…»).
        name: String,
    },
    /// Выделение: число узлов и первые `SELECTION_IDS_MAX` идентификаторов.
    Selection {
        count: u32,
        ids: Vec<String>,
    },
    Tool {
        tool: Tool,
    },
    Cursor {
        cursor: CursorKind,
        angle: f64,
    },
    /// Документ изменён правкой, отменой или повтором.
    Document {
        can_undo: bool,
        can_redo: bool,
        steps: f64,
    },
    Page {
        id: String,
    },
    /// Байты для системного буфера обмена (`web application/x-ferrum`).
    Clipboard {
        #[tsify(type = "Uint8Array")]
        bytes: ByteBuf,
    },
    /// PNG выделения для буфера обмена («Скопировать как PNG»).
    ClipboardPng {
        #[tsify(type = "Uint8Array")]
        bytes: ByteBuf,
    },
    /// Пора прочитать системный буфер и прислать `Paste`.
    PasteRequested,
    /// Выбрать файл изображения и прислать `PlaceImage`.
    RequestImage,
    ToggleUi,
    /// Сохранённый файл `.ferrum`.
    Saved {
        #[tsify(type = "Uint8Array")]
        bytes: ByteBuf,
    },
    /// Пиксели кадра RGBA, строки сверху вниз.
    Pixels {
        backend: Backend,
        width: u32,
        height: u32,
        #[tsify(type = "Uint8Array")]
        rgba: ByteBuf,
    },
    /// Сообщение об ошибке для пользователя.
    Error {
        message: String,
    },
    // ——— Этап 5: панели ———
    /// Окно строк панели слоёв. `reveal` — строка первого выделенного, если
    /// выделение пришло с холста (панель прокручивает к ней).
    Layers {
        start: u32,
        total: u32,
        rows: Vec<LayerRow>,
        reveal: Option<u32>,
    },
    /// Сводка выделения для панели свойств.
    Properties {
        summary: Summary,
    },
    /// Страницы, текущая и её фон (нет — как у холста в теме).
    Pages {
        pages: Vec<PageInfo>,
        current: String,
        background: Option<String>,
    },
    /// Раскладка: все команды, конфликты, сочетания браузера и отличия от
    /// умолчаний в JSON (их интерфейс хранит в настройках).
    Keymap {
        commands: Vec<KeymapEntry>,
        conflicts: Vec<ConflictInfo>,
        reserved: Vec<String>,
        overrides: String,
    },
    /// Сочетание занято: интерфейс предлагает заменить (`Bind` с `replace`).
    BindConflict {
        command: String,
        shortcut: String,
        commands: Vec<String>,
    },
    /// Раскладка импортирована; предупреждения — о пропущенном.
    KeymapImported {
        warnings: Vec<String>,
    },
    Presets {
        presets: Vec<PresetInfo>,
    },
    /// PNG экспорта и имя файла.
    Exported {
        name: String,
        #[tsify(type = "Uint8Array")]
        bytes: ByteBuf,
    },
    /// Имя документа изменилось.
    DocumentName {
        name: String,
    },
}
