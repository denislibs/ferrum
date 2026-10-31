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
/// Размер превью файла для экрана файлов (пиксели): вдвое больше карточки, чтобы
/// на экранах с плотностью 2 оно оставалось чётким.
pub const THUMB_WIDTH: u16 = 480;
pub const THUMB_HEIGHT: u16 = 320;
/// Наибольший текст тестовой команды `TestCreateText` (спека подсистемы 3, раздел 8:
/// `data` ≤ 64 КБ за команду).
pub const MAX_TEXT_DATA_BYTES: usize = 64 * 1024;
/// Наибольшее число семейств в `FontsAvailable` и начертаний у семейства.
pub const MAX_FONT_FAMILIES: usize = 4096;
pub const MAX_FONT_FACES: usize = 1000;
/// Наибольший пример с текстами (`Demo { texts }`).
pub const MAX_DEMO_TEXTS: u32 = 100_000;
/// Наибольшее число пачек журнала изменений при открытии (этап 6): пачка пишется
/// не чаще раза в 2 с, а полный файл — хотя бы раз в 5 минут правок, так что столько
/// набирается, только если полное сохранение много часов не удаётся.
pub const MAX_JOURNAL_BATCHES: usize = 100_000;

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
    Text,
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
    /// Текст в правке и инструмент «Текст».
    Text,
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

/// Источник шрифтов: вшитые в сборку или системные.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[serde(rename_all = "camelCase")]
pub enum FontSource {
    Bundled,
    System,
}

/// Шрифт документа: семейство и начертание.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FontNameDto {
    pub family: String,
    pub style: String,
}

/// Начертание из списка шрифтов: имя, вес, курсив, ширина (у системных шрифтов
/// Chrome их нет — ядро угадывает по имени) и файл (имя в манифесте вшитых или
/// postscript-имя системного) с номером шрифта в коллекции.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Tsify)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FontFaceDto {
    pub style: String,
    #[serde(default)]
    pub weight: Option<f64>,
    #[serde(default)]
    pub italic: Option<bool>,
    #[serde(default)]
    pub stretch: Option<f64>,
    pub file: String,
    #[serde(default)]
    pub index: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Tsify)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FontFamilyDto {
    pub family: String,
    pub faces: Vec<FontFaceDto>,
}

/// Отсутствующий шрифт текущей страницы и сколько текстов им набрано.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Tsify)]
#[serde(rename_all = "camelCase")]
pub struct MissingFontDto {
    pub family: String,
    pub style: String,
    pub nodes: u32,
}

/// Семейство для выбора замены: источник и начертания.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Tsify)]
#[serde(rename_all = "camelCase")]
pub struct FontChoice {
    pub family: String,
    pub source: FontSource,
    pub styles: Vec<String>,
}

/// Режим размера текста тестовой команды.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[serde(rename_all = "camelCase")]
pub enum TextSizingDto {
    AutoWidth,
    AutoHeight,
    Fixed,
}

/// Наибольшая клавиша `TextKey` (`KeyboardEvent.code`) в байтах.
pub const MAX_KEY_CODE_BYTES: usize = 64;
/// Пределы чисел стиля текста из панели (недоверенный ввод).
pub const MAX_FONT_SIZE: f64 = 10_000.0;

/// Вид ввода в текст (`InputEvent.inputType` и составление IME).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[serde(rename_all = "camelCase")]
pub enum TextInputKindDto {
    Insert,
    Paragraph,
    Paste,
    Compose,
    Commit,
    DeleteBackward,
    DeleteForward,
    DeleteWordBackward,
    DeleteWordForward,
    DeleteLineBackward,
    DeleteLineForward,
}

/// Модификаторы клавиши `TextKey`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct KeyMods {
    #[serde(default)]
    pub shift: bool,
    #[serde(default)]
    pub alt: bool,
    #[serde(default)]
    pub ctrl: bool,
    #[serde(default)]
    pub meta: bool,
}

/// Единица интерлиньяжа и трекинга.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[serde(rename_all = "camelCase")]
pub enum SpacingUnit {
    /// Только интерлиньяж: по метрикам шрифта (значение не читается).
    Auto,
    Px,
    Percent,
}

/// Интерлиньяж или трекинг: единица и значение.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Tsify)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SpacingDto {
    pub unit: SpacingUnit,
    pub value: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[serde(rename_all = "camelCase")]
pub enum DecorationDto {
    None,
    Underline,
    Strike,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[serde(rename_all = "camelCase")]
pub enum TextCaseDto {
    None,
    Upper,
    Lower,
    Title,
    SmallCaps,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[serde(rename_all = "camelCase")]
pub enum ListKindDto {
    None,
    Bullet,
    Ordered,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[serde(rename_all = "camelCase")]
pub enum AlignHDto {
    Left,
    Center,
    Right,
    Justify,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[serde(rename_all = "camelCase")]
pub enum AlignVDto {
    Top,
    Middle,
    Bottom,
}

/// Правка стиля символов из секции «Текст»: только известные ключи. Семейство
/// без начертания — начертание то же (если есть в семействе), иначе Regular.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, Tsify)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TextStylePatchDto {
    #[serde(default)]
    pub family: Option<String>,
    #[serde(default)]
    pub style: Option<String>,
    #[serde(default)]
    pub size: Option<f64>,
    #[serde(default)]
    pub line_height: Option<SpacingDto>,
    #[serde(default)]
    pub letter_spacing: Option<SpacingDto>,
    #[serde(default)]
    pub decoration: Option<DecorationDto>,
    #[serde(default)]
    pub case: Option<TextCaseDto>,
    #[serde(default)]
    pub list: Option<ListKindDto>,
}

/// Правка свойств текстовых узлов из секции «Текст».
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, Tsify)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TextPropsPatchDto {
    #[serde(default)]
    pub sizing: Option<TextSizingDto>,
    #[serde(default)]
    pub align_h: Option<AlignHDto>,
    #[serde(default)]
    pub align_v: Option<AlignVDto>,
    #[serde(default)]
    pub paragraph_spacing: Option<f64>,
    #[serde(default)]
    pub paragraph_indent: Option<f64>,
}

/// Значение для панели и признак «Смешанное».
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Tsify)]
#[serde(rename_all = "camelCase")]
pub struct MixedString {
    pub value: String,
    pub mixed: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Tsify)]
#[serde(rename_all = "camelCase")]
pub struct MixedSpacing {
    pub value: SpacingDto,
    pub mixed: bool,
}

/// Сводка стиля для секции «Текст».
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Tsify)]
#[serde(rename_all = "camelCase")]
pub struct TextStyleDto {
    pub family: MixedString,
    pub style: MixedString,
    pub size: Num,
    pub line_height: MixedSpacing,
    pub letter_spacing: MixedSpacing,
    /// Перечисления — строкой: `decoration` (`none`…), `case`, `list`,
    /// `sizing`, `alignH`, `alignV`.
    pub decoration: MixedString,
    pub case: MixedString,
    pub list: MixedString,
    pub sizing: MixedString,
    pub align_h: MixedString,
    pub align_v: MixedString,
    pub paragraph_spacing: Num,
    pub paragraph_indent: Num,
    /// Правится выделенный диапазон (идёт правка текста).
    pub range: bool,
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
    /// Новый документ — страница-пример из `texts` текстовых узлов (фреймы по 100
    /// текстов фиксированного размера): замер «10 000 текстов» (спека подсистемы 3,
    /// раздел 5).
    DemoText {
        texts: u32,
    },
    /// Открыть файл и применить поверх него пачки журнала изменений, записанные
    /// после него, по порядку записи (восстановление после сбоя, этап 6).
    OpenWithJournal {
        #[tsify(type = "Uint8Array")]
        bytes: ByteBuf,
        #[tsify(type = "Uint8Array[]")]
        journal: Vec<ByteBuf>,
    },
    /// Отдать журналу правки с прошлой пачки: ответ — `Journal` (и без правок).
    FlushJournal,
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
    /// Байты документа для «Сохранить копию на диск…»: ответ — событие `Saved`.
    /// Признак `modified` не сбрасывает: копия на диске — не файл в хранилище.
    Save,
    /// Снимок для хранилища: байты документа и (`thumbnail`) превью первой
    /// страницы — ответ `Snapshot`. Признак `modified` снимает только `MarkSaved`
    /// после удачной записи.
    Snapshot {
        thumbnail: bool,
    },
    /// Снимок с ревизией `revision` записан в хранилище: `modified` снимается, если
    /// после снимка правок не было (иначе документ остаётся изменённым).
    MarkSaved {
        revision: f64,
    },
    /// Прочитать пиксели текущего кадра (для тестов): ответ — `Pixels`.
    /// `reference` — тот же кадр, нарисованный `vello_cpu`, для сравнения с GPU.
    ReadPixels {
        reference: bool,
    },
    /// Измерения с Alt в текущем оверлее (для тестов): ответ — `Measurements`.
    ReadMeasurements,
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
    /// Перенос слоёв к строке `target`. `ids` — переносимые, взятые в начале
    /// жеста; без них — текущее выделение целиком (оно может быть больше того,
    /// что видит панель).
    MoveLayers {
        ids: Option<Vec<String>>,
        target: String,
        position: DropPosition,
        /// Без `ids`: версия выделения из события `selection` на начало жеста; если
        /// выделение с тех пор сменилось — переноса нет (как у `EditSelection`).
        selection_version: Option<f64>,
    },
    /// Скрыть или заблокировать (кнопки строки слоя).
    SetLayerFlags {
        ids: Vec<String>,
        visible: Option<bool>,
        locked: Option<bool>,
    },
    /// Правка из панели свойств. `ids` — узлы, которые правились с начала
    /// жеста (поле могло потерять выделение); без них — текущее выделение.
    EditSelection {
        edit: Edit,
        phase: EditPhase,
        ids: Option<Vec<String>>,
        /// Без `ids` (выделение больше предела id): версия выделения из события
        /// `selection` на начало жеста; если выделение с тех пор сменилось — правки нет.
        selection_version: Option<f64>,
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
        /// Имя из хранилища при открытии: не правка пользователя — документ не
        /// становится изменённым (файл не пересохраняется только из-за открытия).
        #[serde(default)]
        quiet: Option<bool>,
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
    // ——— Подсистема 3, этап 1: шрифты ———
    /// Список шрифтов источника: вшитые (манифест сборки — его читает worker ядра)
    /// или системные (Chrome — `queryLocalFonts`, десктоп — сканирование каталогов).
    FontsAvailable {
        source: FontSource,
        families: Vec<FontFamilyDto>,
    },
    /// Байты файла шрифта в ответ на `FontRequest`; без байтов — файл получить не
    /// удалось (шрифт считается отсутствующим).
    LoadFont {
        family: String,
        file: String,
        #[serde(default)]
        #[tsify(type = "Uint8Array | undefined")]
        bytes: Option<ByteBuf>,
    },
    /// Заменить шрифт во всех текстах текущей страницы одним шагом отмены.
    ReplaceMissingFont {
        from: FontNameDto,
        to: FontNameDto,
    },
    /// Только тестовый режим (`init.test`; worker отвергает её без него): текстовый
    /// узел в точке мира — инструмента T до этапа 2 нет. Ждёт шрифтов, если они
    /// ещё грузятся.
    TestCreateText {
        x: f64,
        y: f64,
        text: String,
        #[serde(default)]
        font: Option<FontNameDto>,
        #[serde(default)]
        size: Option<f64>,
        #[serde(default)]
        sizing: Option<TextSizingDto>,
        #[serde(default)]
        width: Option<f64>,
        #[serde(default)]
        height: Option<f64>,
    },
    // ——— Подсистема 3, этап 2: правка текста ———
    /// Войти в правку текста (весь текст выделен).
    TextEditStart {
        id: String,
    },
    /// Выйти из правки (как Esc).
    TextEditEnd,
    /// Ввод из поля: набор, абзац, удаление, вставка обычного текста (вне правки
    /// — новый текст), составление IME. `data` — не больше 64 КБ.
    TextInput {
        kind: TextInputKindDto,
        #[serde(default)]
        data: Option<String>,
    },
    /// Клавиша в правке: ход курсора, удаление, отмена, форматирование
    /// (`key` — `KeyboardEvent.code`).
    TextKey {
        key: String,
        #[serde(default)]
        mods: KeyMods,
    },
    /// Стиль символов: в правке — выделенного диапазона (у курсора — следующего
    /// ввода), иначе — всего текста узлов `ids` (без них — выделения).
    SetTextStyle {
        patch: TextStylePatchDto,
        phase: EditPhase,
        #[serde(default)]
        ids: Option<Vec<String>>,
    },
    /// Свойства текстовых узлов (режим размера, выравнивание, интервалы абзацев).
    SetTextProps {
        patch: TextPropsPatchDto,
        phase: EditPhase,
        #[serde(default)]
        ids: Option<Vec<String>>,
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
        /// Версия выделения редактора: растёт при каждом изменении.
        version: f64,
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
    /// Байты для системного буфера обмена (`web application/x-ferrum`); у
    /// фрагмента текста в правке — и обычный текст для `text/plain`.
    Clipboard {
        #[tsify(type = "Uint8Array")]
        bytes: ByteBuf,
        #[serde(default)]
        #[tsify(type = "string | undefined")]
        plain: Option<String>,
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
    /// Снимок для хранилища: ревизия правок, имя документа, байты `.ferrum` и
    /// PNG превью первой страницы (нет — нечего показать или превью не просили).
    Snapshot {
        revision: f64,
        name: String,
        #[tsify(type = "Uint8Array")]
        bytes: ByteBuf,
        #[tsify(type = "Uint8Array | undefined")]
        thumbnail: Option<ByteBuf>,
        /// Превью не строили или не смогли построить: прежнее в хранилище оставить
        /// (без превью и без этого признака — страница пуста, превью убрать).
        keep_thumbnail: bool,
        /// Номер последней пачки журнала, вошедшей в снимок: после записи снимка
        /// пачки до неё включительно не нужны.
        journal: f64,
    },
    /// Ответ на `FlushJournal` (приходит всегда). Пачка журнала изменений: номер
    /// (растёт с каждой непустой пачкой) и байты; без байтов и без `failed` —
    /// правок с прошлой пачки нет, всё отдано. `failed` — пачку собрать не удалось
    /// (рядом идёт `Error`): правки остаются несохранёнными в журнале, номер прежний.
    Journal {
        batch: f64,
        #[tsify(type = "Uint8Array | undefined")]
        bytes: Option<ByteBuf>,
        failed: bool,
    },
    /// Перед `Opened`: поверх файла применены пачки журнала — правки, которые не
    /// успели попасть в файл (документ изменён).
    Recovered {
        batches: u32,
    },
    /// Команда «Сохранить» (Ctrl/Cmd+S): записать файл в хранилище.
    SaveRequested,
    /// Измерения с Alt в оверлее: значения расстояний по порядку и число
    /// пунктирных направляющих.
    Measurements {
        values: Vec<f64>,
        guides: u32,
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
    /// Поверхность GPU потеряна (устройство WebGPU, контекст WebGL2): worker создаёт
    /// её заново; до `SurfaceRestored` кадров нет.
    SurfaceLost {
        backend: Backend,
        message: String,
    },
    /// Поверхность создана заново, кадры идут.
    SurfaceRestored {
        backend: Backend,
    },
    /// Пересоздать не удалось (или потери повторяются): интерфейс перезапускает ядро
    /// с бэкендом `next`, документ — из файла и журнала.
    SurfaceFailed {
        next: Backend,
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
    /// Документ изменён с последнего открытия, создания или сохранения — любой
    /// правкой, и той, что не в истории отмены (страницы, фон, имя файла).
    /// Приходит, когда признак сменился.
    Modified {
        modified: bool,
    },
    // ——— Подсистема 3, этап 1: шрифты ———
    /// Ядру нужны байты файлов семейства: вшитые качает worker ядра, системные —
    /// основной поток (`LoadFont` на каждый файл).
    FontRequest {
        family: String,
        source: FontSource,
        files: Vec<String>,
    },
    /// Отсутствующие шрифты текущей страницы; приходит, когда список сменился.
    MissingFonts {
        list: Vec<MissingFontDto>,
    },
    /// Известные семейства для выбора замены; приходит, когда список сменился.
    FontCatalog {
        families: Vec<FontChoice>,
    },
    /// Сообщение для журнала ошибок (консоль), не для уведомления: неразборчивый
    /// файл шрифта, пропущенное семейство.
    Log {
        message: String,
    },
    // ——— Подсистема 3, этап 2: правка текста ———
    /// Правка текста: идёт ли, прямоугольник курсора в CSS-пикселях холста
    /// `[x, y, w, h]` (поле ввода и окно IME встают под него) и выделение
    /// `[якорь, курсор]` в символах. Приходит, когда что-то из этого сменилось.
    TextCaret {
        editing: bool,
        rect: Option<[f64; 4]>,
        selection: Option<[u32; 2]>,
        /// Новый текст (инструмент T) ещё не в документе.
        draft: bool,
    },
    /// Стиль для секции «Текст»; `None` — текстов не выделено.
    TextStyleSummary {
        summary: Option<TextStyleDto>,
    },
    /// Семейства текстов текущей страницы (группа «В документе» выбора шрифта).
    PageFonts {
        families: Vec<String>,
    },
}
