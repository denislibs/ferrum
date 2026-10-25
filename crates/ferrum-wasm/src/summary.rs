//! Сводка выделения для панели свойств (этап 5). Worker сводит свойства всех
//! выделенных узлов сам и отдаёт готовые значения: у одинаковых — значение, у
//! разных — «Смешанные» (`mixed`, значение первого узла — для перетаскивания
//! подписи). Поле, которое не относится ни к одному выделенному узлу (радиус у
//! эллипсов, обрезка не у фреймов), отсутствует (`None`). Считается при смене
//! выделения или документа, а не в каждом кадре: O(выделенных).

use ferrum_doc::{Fill, FillKind, NodeId, NodeKind, ScaleMode, Stroke, StrokeAlign};
use ferrum_editor::{field_applies, panel_pose, NumField};
use ferrum_scene::Scene;
use serde::{Deserialize, Serialize};
use tsify::Tsify;

use crate::layers::{NodeType, MAX_ROW_NAME_CHARS};

/// Сколько заливок и обводок уходит в сводке (длиннее — «Смешанные») и
/// принимается в команде.
pub const MAX_PAINTS: usize = 64;

/// Числовое поле: значение (у «Смешанных» — первого узла) и признак различия.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Tsify)]
#[serde(rename_all = "camelCase")]
pub struct Num {
    pub value: f64,
    pub mixed: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Tsify)]
#[serde(rename_all = "camelCase")]
pub struct Flag {
    pub value: bool,
    pub mixed: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[serde(rename_all = "camelCase")]
pub enum PaintKind {
    Solid,
    Image,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[serde(rename_all = "camelCase")]
pub enum ScaleModeDto {
    Fill,
    Fit,
    Tile,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[serde(rename_all = "camelCase")]
pub enum Align {
    Inside,
    Center,
    Outside,
}

/// Заливка для панели и команды `setFills`. Цвет — `#rrggbbaa`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Tsify)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FillDto {
    pub kind: PaintKind,
    pub color: String,
    pub opacity: f64,
    pub visible: bool,
    /// Только у изображения: хэш байтов и режим. Панель показывает их, но не меняет.
    #[serde(default)]
    pub image_hash: Option<String>,
    #[serde(default)]
    pub scale_mode: Option<ScaleModeDto>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Tsify)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StrokeDto {
    pub color: String,
    pub width: f64,
    pub align: Align,
}

/// Заливки или обводки выделения: список, если у всех одинаковый.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Tsify)]
#[serde(rename_all = "camelCase")]
pub struct Paints<T> {
    pub mixed: bool,
    pub list: Vec<T>,
}

/// Сводка выделения.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Tsify)]
#[serde(rename_all = "camelCase")]
pub struct Summary {
    pub count: u32,
    /// Тип, если у всех выделенных он один.
    pub kind: Option<NodeType>,
    /// Имя единственного выделенного (для экспорта и заголовка).
    pub name: Option<String>,
    pub x: Num,
    pub y: Num,
    pub w: Num,
    pub h: Option<Num>,
    pub rotation: Num,
    pub opacity: Num,
    pub corner_radius: Option<Num>,
    pub clip_content: Option<Flag>,
    pub fills: Option<Paints<FillDto>>,
    pub strokes: Option<Paints<StrokeDto>>,
}

impl Summary {
    pub fn empty() -> Summary {
        let zero = Num {
            value: 0.0,
            mixed: false,
        };
        Summary {
            count: 0,
            kind: None,
            name: None,
            x: zero,
            y: zero,
            w: zero,
            h: None,
            rotation: zero,
            opacity: Num {
                value: 1.0,
                mixed: false,
            },
            corner_radius: None,
            clip_content: None,
            fills: None,
            strokes: None,
        }
    }
}

pub fn fill_dto(f: &Fill) -> FillDto {
    FillDto {
        kind: match f.kind {
            FillKind::Solid => PaintKind::Solid,
            FillKind::Image => PaintKind::Image,
        },
        color: f.color.to_hex(),
        opacity: f.opacity,
        visible: f.visible,
        image_hash: f.image_hash.clone(),
        scale_mode: (f.kind == FillKind::Image).then_some(match f.scale_mode {
            ScaleMode::Fill => ScaleModeDto::Fill,
            ScaleMode::Fit => ScaleModeDto::Fit,
            ScaleMode::Tile => ScaleModeDto::Tile,
        }),
    }
}

pub fn stroke_dto(s: &Stroke) -> StrokeDto {
    StrokeDto {
        color: s.color.to_hex(),
        width: s.width,
        align: match s.align {
            StrokeAlign::Inside => Align::Inside,
            StrokeAlign::Center => Align::Center,
            StrokeAlign::Outside => Align::Outside,
        },
    }
}

/// Накопитель поля: первое значение и было ли отличие.
struct Acc<T> {
    first: Option<T>,
    mixed: bool,
}

impl<T> Default for Acc<T> {
    fn default() -> Self {
        Acc {
            first: None,
            mixed: false,
        }
    }
}

impl<T: PartialEq> Acc<T> {
    fn add(&mut self, v: T) {
        match &self.first {
            None => self.first = Some(v),
            Some(f) => {
                if !self.mixed && *f != v {
                    self.mixed = true;
                }
            }
        }
    }
}

impl Acc<f64> {
    fn num(self) -> Option<Num> {
        self.first.map(|value| Num {
            value,
            mixed: self.mixed,
        })
    }
}

/// Сводка по выделенным узлам (неизвестные пропускаются).
pub fn summarize(scene: &Scene, ids: &[NodeId]) -> Summary {
    let mut count = 0u32;
    let mut kind: Acc<NodeKind> = Acc::default();
    let (mut x, mut y, mut w, mut h, mut rot) = (
        Acc::default(),
        Acc::default(),
        Acc::default(),
        Acc::default(),
        Acc::default(),
    );
    let mut opacity: Acc<f64> = Acc::default();
    let mut radius: Acc<f64> = Acc::default();
    let mut clip: Acc<bool> = Acc::default();
    let mut fills: Acc<&[Fill]> = Acc::default();
    let mut strokes: Acc<&[Stroke]> = Acc::default();
    let mut name = None;
    for id in ids {
        let Some(slot) = scene.slot(*id) else {
            continue;
        };
        // Без паники на устаревшем слоте: узел мог исчезнуть между выделением и сводкой.
        let Some(n) = scene.node_at(slot) else {
            continue;
        };
        let p = &n.props;
        count += 1;
        kind.add(p.kind);
        let pose = panel_pose(scene, slot);
        x.add(pose.x);
        y.add(pose.y);
        w.add(pose.w);
        if field_applies(NumField::H, p.kind) {
            h.add(pose.h);
        }
        rot.add(pose.rotation);
        opacity.add(p.opacity);
        if field_applies(NumField::CornerRadius, p.kind) {
            radius.add(p.corner_radius);
        }
        if p.kind == NodeKind::Frame {
            clip.add(p.clip_content);
        }
        if p.kind != NodeKind::Group && p.kind != NodeKind::Line {
            fills.add(&p.fills);
        }
        if p.kind != NodeKind::Group {
            strokes.add(&p.strokes);
        }
        if count == 1 {
            name = Some(p.name.chars().take(MAX_ROW_NAME_CHARS).collect::<String>());
        }
    }
    if count == 0 {
        return Summary::empty();
    }
    let zero = Num {
        value: 0.0,
        mixed: false,
    };
    Summary {
        count,
        kind: (!kind.mixed)
            .then(|| kind.first.map(NodeType::from))
            .flatten(),
        name: (count == 1).then_some(name).flatten(),
        x: x.num().unwrap_or(zero),
        y: y.num().unwrap_or(zero),
        w: w.num().unwrap_or(zero),
        h: h.num(),
        rotation: rot.num().unwrap_or(zero),
        opacity: opacity.num().unwrap_or(zero),
        corner_radius: radius.num(),
        clip_content: clip.first.map(|value| Flag {
            value,
            mixed: clip.mixed,
        }),
        // Список длиннее `MAX_PAINTS` не уходит в панель целиком; правка списка
        // заменила бы его усечённым и потеряла бы остальное. Поэтому он — как
        // «Смешанные»: панель предлагает заменить его явно («+»).
        fills: fills.first.map(|list| {
            let whole = !fills.mixed && list.len() <= MAX_PAINTS;
            Paints {
                mixed: !whole,
                list: if whole {
                    list.iter().map(fill_dto).collect()
                } else {
                    vec![]
                },
            }
        }),
        strokes: strokes.first.map(|list| {
            let whole = !strokes.mixed && list.len() <= MAX_PAINTS;
            Paints {
                mixed: !whole,
                list: if whole {
                    list.iter().map(stroke_dto).collect()
                } else {
                    vec![]
                },
            }
        }),
    }
}

/// Ключ актуальности сводки: экземпляр сцены, счётчик изменений (правки и превью
/// жеста) и версия выделения. Пока он не изменился, сводка не пересчитывается.
type CacheKey = (u64, u64, u64);

/// Кэш сводки: во время жеста с тысячей выделенных узлов сводка считается только
/// когда что-то поменялось, а не в каждом кадре.
#[derive(Default)]
pub struct SummaryCache {
    key: Option<CacheKey>,
    value: Option<Summary>,
}

impl SummaryCache {
    pub fn new() -> SummaryCache {
        SummaryCache::default()
    }

    /// Сводка выделения; `selection_version` — `Editor::selection_version`.
    pub fn get(&mut self, scene: &Scene, ids: &[NodeId], selection_version: u64) -> &Summary {
        let key = (scene.generation(), scene.change_counter(), selection_version);
        if self.key != Some(key) || self.value.is_none() {
            self.value = Some(summarize(scene, ids));
            self.key = Some(key);
        }
        self.value.as_ref().expect("сводка только что посчитана")
    }
}
