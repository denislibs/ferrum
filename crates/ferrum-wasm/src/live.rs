//! Блок живого состояния: то, что основной поток читает в каждом
//! `requestAnimationFrame` без сообщений — значения выделения во время
//! перетаскивания, камера, статистика кадра. Все поля — `f64` (одна ячейка
//! `Float64Array`); логические — 0 или 1, бэкенд — индекс в `layout::BACKENDS`.

use crate::layout::LIVE_SLOTS;

macro_rules! live_fields {
    ($($(#[doc = $doc:literal])* $name:ident => $variant:ident, $ts:literal;)+) => {
        /// Поле живого состояния; значение — номер ячейки. Имена — как у констант
        /// TypeScript (`Live.FRAME_MS`), чтобы их было легко искать в обоих языках.
        #[allow(non_camel_case_types)]
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        pub enum LiveField {
            $($(#[doc = $doc])* $variant,)+
        }

        impl LiveField {
            pub const ALL: &'static [LiveField] = &[$(LiveField::$variant,)+];

            /// Имя константы в TypeScript (`Live.FRAME_MS`).
            pub fn ts_name(self) -> &'static str {
                match self {
                    $(LiveField::$variant => stringify!($variant),)+
                }
            }

            /// Имя поля объекта в TypeScript (`live.frameMs`).
            pub fn js_name(self) -> &'static str {
                match self {
                    $(LiveField::$variant => $ts,)+
                }
            }
        }

        #[derive(Debug, Clone, Copy, Default, PartialEq)]
        pub struct LiveState {
            $($(#[doc = $doc])* pub $name: f64,)+
        }

        impl LiveState {
            /// Значения по ячейкам; лишние ячейки — нули.
            pub fn to_slots(&self) -> [f64; LIVE_SLOTS] {
                let mut out = [0.0; LIVE_SLOTS];
                $(out[LiveField::$variant as usize] = self.$name;)+
                out
            }

            /// Обратно из ячеек; недостающие ячейки — нули.
            pub fn from_slots(slots: &[f64]) -> LiveState {
                let get = |f: LiveField| slots.get(f as usize).copied().unwrap_or(0.0);
                LiveState {
                    $($name: get(LiveField::$variant),)+
                }
            }
        }
    };
}

live_fields! {
    /// Нарисовано кадров с начала работы.
    frames => FRAMES, "frames";
    /// Время последнего кадра в worker'е (рисование и отправка в GPU), мс.
    frame_ms => FRAME_MS, "frameMs";
    /// Медиана и 95-й процентиль времени кадра по последним 120 кадрам, мс.
    frame_p50 => FRAME_P50, "frameP50";
    frame_p95 => FRAME_P95, "frameP95";
    /// Нарисовано кадров за последнюю секунду.
    fps => FPS, "fps";
    /// Активный бэкенд рендера (`layout::BACKENDS`).
    backend => BACKEND, "backend";
    /// Выделение: число узлов и значения панели «Положение» — у одного узла его
    /// x, y (у родителя), ширина, высота и поворот; у нескольких — общая рамка
    /// в мире.
    selection_count => SELECTION_COUNT, "selectionCount";
    selection_x => SELECTION_X, "selectionX";
    selection_y => SELECTION_Y, "selectionY";
    selection_w => SELECTION_W, "selectionW";
    selection_h => SELECTION_H, "selectionH";
    selection_rotation => SELECTION_ROTATION, "selectionRotation";
    /// Камера: центр в мире и зум.
    camera_x => CAMERA_X, "cameraX";
    camera_y => CAMERA_Y, "cameraY";
    zoom => ZOOM, "zoom";
    /// Холст в CSS-пикселях и `devicePixelRatio`, который применил worker.
    viewport_w => VIEWPORT_W, "viewportW";
    viewport_h => VIEWPORT_H, "viewportH";
    dpr => DPR, "dpr";
    /// Размер холста в пикселях устройства.
    device_w => DEVICE_W, "deviceW";
    device_h => DEVICE_H, "deviceH";
    /// Идёт жест (кнопка нажата).
    busy => BUSY, "busy";
    /// Шагов истории, созданных редактором; можно ли отменить и повторить.
    steps => STEPS, "steps";
    can_undo => CAN_UNDO, "canUndo";
    can_redo => CAN_REDO, "canRedo";
    /// Узлов в сцене текущей страницы.
    nodes => NODES, "nodes";
    /// Записей ввода принято и пропущено (непонятных).
    inputs => INPUTS, "inputs";
    skipped => SKIPPED, "skipped";
    /// Картинок в атласе GPU и картинок, которым не хватило места.
    atlas_images => ATLAS_IMAGES, "atlasImages";
    atlas_misses => ATLAS_MISSES, "atlasMisses";
}
