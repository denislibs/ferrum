//! Сводка выделения для панели свойств (этап 5). Worker сводит свойства всех
//! выделенных узлов сам и отдаёт готовые значения: у одинаковых — значение, у
//! разных — «Смешанные» (`mixed`, значение первого узла — для перетаскивания
//! подписи). Поле, которое не относится ни к одному выделенному узлу (радиус у
//! эллипсов, обрезка не у фреймов), отсутствует (`None`). Считается при смене
//! выделения или документа, а не в каждом кадре: O(выделенных).

use ferrum_doc::{
    Fill, FillKind, NodeId, NodeKind, ScaleMode, Stroke, StrokeAlign, StrokeCap, StrokeJoin,
};
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

/// Окончание открытого конца обводки (спека подсистемы 3, раздел 9).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[serde(rename_all = "camelCase")]
pub enum StrokeCapDto {
    None,
    Round,
    Square,
    ArrowLines,
    ArrowTriangle,
    Circle,
    Diamond,
}

/// Соединение сегментов обводки.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Tsify)]
#[serde(rename_all = "camelCase")]
pub enum StrokeJoinDto {
    Miter,
    Round,
    Bevel,
}

pub fn cap_dto(c: StrokeCap) -> StrokeCapDto {
    match c {
        StrokeCap::None => StrokeCapDto::None,
        StrokeCap::Round => StrokeCapDto::Round,
        StrokeCap::Square => StrokeCapDto::Square,
        StrokeCap::ArrowLines => StrokeCapDto::ArrowLines,
        StrokeCap::ArrowTriangle => StrokeCapDto::ArrowTriangle,
        StrokeCap::Circle => StrokeCapDto::Circle,
        StrokeCap::Diamond => StrokeCapDto::Diamond,
    }
}

pub fn cap_of(c: StrokeCapDto) -> StrokeCap {
    match c {
        StrokeCapDto::None => StrokeCap::None,
        StrokeCapDto::Round => StrokeCap::Round,
        StrokeCapDto::Square => StrokeCap::Square,
        StrokeCapDto::ArrowLines => StrokeCap::ArrowLines,
        StrokeCapDto::ArrowTriangle => StrokeCap::ArrowTriangle,
        StrokeCapDto::Circle => StrokeCap::Circle,
        StrokeCapDto::Diamond => StrokeCap::Diamond,
    }
}

pub fn join_dto(j: StrokeJoin) -> StrokeJoinDto {
    match j {
        StrokeJoin::Miter => StrokeJoinDto::Miter,
        StrokeJoin::Round => StrokeJoinDto::Round,
        StrokeJoin::Bevel => StrokeJoinDto::Bevel,
    }
}

pub fn join_of(j: StrokeJoinDto) -> StrokeJoin {
    match j {
        StrokeJoinDto::Miter => StrokeJoin::Miter,
        StrokeJoinDto::Round => StrokeJoin::Round,
        StrokeJoinDto::Bevel => StrokeJoin::Bevel,
    }
}

/// Стиль обводки выделения: значение поля — общее у всех, `None` — «Смешанные».
/// Окончания показываются, только если у кого-то есть открытые концы (линия,
/// открытый путь вектора), соединение — если есть углы.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Tsify)]
#[serde(rename_all = "camelCase")]
pub struct StrokeStyleDto {
    pub ends: bool,
    pub corners: bool,
    pub cap_start: Option<StrokeCapDto>,
    pub cap_end: Option<StrokeCapDto>,
    pub join: Option<StrokeJoinDto>,
    pub miter_limit: Num,
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
    /// Окончания и соединения обводки (у групп и текста их нет).
    pub stroke_style: Option<StrokeStyleDto>,
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
            stroke_style: None,
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

/// Допуск сравнения чисел: относительный, от большего из модулей (но не меньше
/// единицы). Панель показывает числа с точностью заведомо грубее, поэтому
/// различие меньше допуска — шум вычислений (`0.1 + 0.2` и `0.3`), а не «Смешанные».
pub const MIXED_EPS: f64 = 1e-6;

/// Равны ли числа с допуском `MIXED_EPS`.
fn near(a: f64, b: f64) -> bool {
    (a - b).abs() <= MIXED_EPS * a.abs().max(b.abs()).max(1.0)
}

/// Равны ли углы в градусах с учётом оборота: 360 и 0, 180 и -180 — одно и то же.
fn near_angle(a: f64, b: f64) -> bool {
    near((a - b + 180.0).rem_euclid(360.0) - 180.0, 0.0)
}

fn fills_eq(a: &[Fill], b: &[Fill]) -> bool {
    a.len() == b.len()
        && a.iter().zip(b).all(|(x, y)| {
            x.kind == y.kind
                && x.color == y.color
                && x.image_hash == y.image_hash
                && x.scale_mode == y.scale_mode
                && x.visible == y.visible
                && near(x.opacity, y.opacity)
        })
}

fn strokes_eq(a: &[Stroke], b: &[Stroke]) -> bool {
    a.len() == b.len()
        && a.iter()
            .zip(b)
            .all(|(x, y)| x.color == y.color && x.align == y.align && near(x.width, y.width))
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

impl<T> Acc<T> {
    /// Добавить значение; `eq` решает, считать ли его равным первому.
    fn add_by(&mut self, v: T, eq: impl Fn(&T, &T) -> bool) {
        match &self.first {
            None => self.first = Some(v),
            Some(f) => {
                if !self.mixed && !eq(f, &v) {
                    self.mixed = true;
                }
            }
        }
    }
}

impl<T: PartialEq> Acc<T> {
    fn add(&mut self, v: T) {
        self.add_by(v, |a, b| a == b);
    }
}

impl Acc<f64> {
    fn add_num(&mut self, v: f64) {
        self.add_by(v, |a, b| near(*a, *b));
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
    let mut x: Acc<f64> = Acc::default();
    let mut y: Acc<f64> = Acc::default();
    let mut w: Acc<f64> = Acc::default();
    let mut h: Acc<f64> = Acc::default();
    let mut rot: Acc<f64> = Acc::default();
    let mut opacity: Acc<f64> = Acc::default();
    let mut radius: Acc<f64> = Acc::default();
    let mut clip: Acc<bool> = Acc::default();
    let mut fills: Acc<&[Fill]> = Acc::default();
    let mut strokes: Acc<&[Stroke]> = Acc::default();
    let mut cap_start: Acc<StrokeCap> = Acc::default();
    let mut cap_end: Acc<StrokeCap> = Acc::default();
    let mut join: Acc<StrokeJoin> = Acc::default();
    let mut miter: Acc<f64> = Acc::default();
    let (mut ends, mut corners) = (false, false);
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
        x.add_num(pose.x);
        y.add_num(pose.y);
        w.add_num(pose.w);
        if field_applies(NumField::H, p.kind) {
            h.add_num(pose.h);
        }
        rot.add_by(pose.rotation, |a, b| near_angle(*a, *b));
        opacity.add_num(p.opacity);
        if field_applies(NumField::CornerRadius, p.kind) {
            radius.add_num(p.corner_radius);
        }
        if p.kind == NodeKind::Frame {
            clip.add(p.clip_content);
        }
        if p.kind != NodeKind::Group && p.kind != NodeKind::Line {
            fills.add_by(&p.fills, |a, b| fills_eq(a, b));
        }
        if p.kind != NodeKind::Group {
            strokes.add_by(&p.strokes, |a, b| strokes_eq(a, b));
        }
        if !matches!(p.kind, NodeKind::Group | NodeKind::Text) {
            let s = &p.stroke_style;
            join.add(s.join);
            miter.add_num(s.miter_limit);
            match p.kind {
                NodeKind::Line => {
                    ends = true;
                    cap_start.add(s.cap_start);
                    cap_end.add(s.cap_end);
                }
                NodeKind::Vector => {
                    let shape = ferrum_scene::vector::network_shape(p);
                    corners = true;
                    if !shape.ends.is_empty() {
                        ends = true;
                        cap_start.add(s.cap_start);
                        cap_end.add(s.cap_end);
                    }
                }
                NodeKind::Ellipse => {}
                _ => corners |= p.corner_radius <= 0.0,
            }
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
        stroke_style: join.first.map(|j| StrokeStyleDto {
            ends,
            corners,
            cap_start: cap_start.first.filter(|_| !cap_start.mixed).map(cap_dto),
            cap_end: cap_end.first.filter(|_| !cap_end.mixed).map(cap_dto),
            join: (!join.mixed).then(|| join_dto(j)),
            miter_limit: miter.num().unwrap_or(Num {
                value: ferrum_doc::DEFAULT_MITER_LIMIT,
                mixed: false,
            }),
        }),
    }
}
