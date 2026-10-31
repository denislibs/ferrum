//! Векторная сеть (спека подсистемы 3, раздел 9): точки, сегменты между ними и
//! области — петли сегментов со своим правилом заливки и, если заданы, своими
//! заливками. Как в Figma, сеть не обязана быть набором контуров: в точке
//! сходится сколько угодно сегментов.
//!
//! В документе сеть — одно значение узла (ключ `vector`, двоичная форма ниже), а
//! не дерево контейнеров Loro: правка сети заменяет значение целиком (как список
//! заливок), а 100 000 точек в контейнерах раздули бы файл в разы. Координаты —
//! в осях узла; рамка узла (`w × h`) — границы сети, а ресайз узла растягивает
//! сеть под рамку (`ferrum_scene::vector`).
//!
//! Данные из файла и буфера обмена не доверенные: `decode` проверяет пределы,
//! числа, индексы и то, что каждая петля области — замкнутый цикл сегментов.

use crate::binary::{Reader, Writer};
use crate::scene_cache::{read_fill, write_fill};
use crate::types::{Fill, FillKind};

/// Предел точек сети (спека 9).
pub const MAX_VECTOR_POINTS: usize = 100_000;
/// Предел сегментов: в сети точка может соединяться с любым числом соседей, но
/// больше двух сегментов на точку в среднем не бывает ни у контуров, ни у сеток.
pub const MAX_VECTOR_SEGMENTS: usize = 2 * MAX_VECTOR_POINTS;
/// Предел областей и петель (суммарно по сети).
pub const MAX_VECTOR_LOOPS: usize = 10_000;
/// Предел заливок одной области (как у узла).
pub const MAX_REGION_FILLS: usize = 64;
/// Предел координат и ручек (как у узлов сцены, `ferrum_scene::shape::COORD_LIMIT`).
pub const MAX_VECTOR_COORD: f64 = 1.0e9;
/// Версия двоичной формы.
const FORMAT: u8 = 1;

/// Режим ручек точки, как в Figma: без ручек, зеркально по углу (длины свои),
/// зеркально по углу и длине, независимые.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HandleMode {
    None,
    MirrorAngle,
    MirrorAll,
    Free,
}

const HANDLE_MODES: [HandleMode; 4] = [
    HandleMode::None,
    HandleMode::MirrorAngle,
    HandleMode::MirrorAll,
    HandleMode::Free,
];

/// Правило заливки области.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FillRule {
    NonZero,
    EvenOdd,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VectorPoint {
    pub x: f64,
    pub y: f64,
    pub handle_mode: HandleMode,
    /// Радиус скругления угла в этой точке (хранится; рисование — позже, раздел
    /// «Что этап 3 не делает» плана).
    pub corner_radius: f64,
}

impl VectorPoint {
    pub fn new(x: f64, y: f64) -> VectorPoint {
        VectorPoint {
            x,
            y,
            handle_mode: HandleMode::None,
            corner_radius: 0.0,
        }
    }
}

/// Сегмент от точки `start` к точке `end`. Ручки — смещения от своих точек:
/// нулевые обе — отрезок, иначе кубическая кривая
/// `start → start + tangent_start → end + tangent_end → end`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VectorSegment {
    pub start: u32,
    pub end: u32,
    pub tangent_start: [f64; 2],
    pub tangent_end: [f64; 2],
}

impl VectorSegment {
    pub fn line(start: u32, end: u32) -> VectorSegment {
        VectorSegment {
            start,
            end,
            tangent_start: [0.0; 2],
            tangent_end: [0.0; 2],
        }
    }

    /// Прямой ли сегмент (обе ручки нулевые).
    pub fn is_straight(&self) -> bool {
        self.tangent_start == [0.0; 2] && self.tangent_end == [0.0; 2]
    }
}

/// Область: петли (каждая — цикл сегментов по индексам) с правилом заливки.
/// `fills = None` — заливки узла (как `VectorRegion.fills` в Figma).
#[derive(Debug, Clone, PartialEq)]
pub struct VectorRegion {
    pub rule: FillRule,
    pub loops: Vec<Vec<u32>>,
    pub fills: Option<Vec<Fill>>,
}

/// Сеть узла `Vector`.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct VectorNetwork {
    pub points: Vec<VectorPoint>,
    pub segments: Vec<VectorSegment>,
    pub regions: Vec<VectorRegion>,
}

/// Почему сеть не принята.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum VectorError {
    #[error("в векторе больше 100 000 точек")]
    TooManyPoints,
    #[error("в векторе слишком много сегментов или областей")]
    TooLarge,
    #[error("векторная сеть повреждена: {0}")]
    Invalid(&'static str),
}

fn finite_coord(v: f64) -> bool {
    v.is_finite() && v.abs() <= MAX_VECTOR_COORD
}

impl VectorNetwork {
    /// Степень точки: сколько сегментов в ней сходится.
    pub fn degrees(&self) -> Vec<u32> {
        let mut d = vec![0u32; self.points.len()];
        for s in &self.segments {
            d[s.start as usize] += 1;
            d[s.end as usize] += 1;
        }
        d
    }

    /// Точки петли по порядку обхода (первая не повторяется в конце); `None` —
    /// сегменты не образуют замкнутый цикл.
    pub fn loop_points(&self, segs: &[u32]) -> Option<Vec<u32>> {
        let first = self.segments.get(*segs.first()? as usize)?;
        // Направление первого сегмента — то, при котором второй к нему примыкает.
        'orient: for (from, to) in [(first.start, first.end), (first.end, first.start)] {
            let mut out = vec![from];
            let mut cur = to;
            for &i in &segs[1..] {
                let s = self.segments.get(i as usize)?;
                out.push(cur);
                cur = if s.start == cur {
                    s.end
                } else if s.end == cur {
                    s.start
                } else {
                    continue 'orient;
                };
            }
            if cur == from && out.len() >= 2 {
                return Some(out);
            }
        }
        None
    }

    /// Проверка пределов и связности: числа конечные, индексы в сети, петли
    /// областей замкнуты, заливки областей — только цвет.
    pub fn validate(&self) -> Result<(), VectorError> {
        if self.points.len() > MAX_VECTOR_POINTS {
            return Err(VectorError::TooManyPoints);
        }
        let loops: usize = self.regions.iter().map(|r| r.loops.len()).sum();
        if self.segments.len() > MAX_VECTOR_SEGMENTS || loops > MAX_VECTOR_LOOPS {
            return Err(VectorError::TooLarge);
        }
        for p in &self.points {
            if !finite_coord(p.x) || !finite_coord(p.y) {
                return Err(VectorError::Invalid("координата точки"));
            }
            if !(p.corner_radius.is_finite() && (0.0..=MAX_VECTOR_COORD).contains(&p.corner_radius))
            {
                return Err(VectorError::Invalid("радиус точки"));
            }
        }
        let n = self.points.len() as u32;
        for s in &self.segments {
            if s.start >= n || s.end >= n || s.start == s.end {
                return Err(VectorError::Invalid("концы сегмента"));
            }
            if !s
                .tangent_start
                .iter()
                .chain(&s.tangent_end)
                .all(|v| finite_coord(*v))
            {
                return Err(VectorError::Invalid("ручка сегмента"));
            }
        }
        for r in &self.regions {
            if r.loops.is_empty() {
                return Err(VectorError::Invalid("пустая область"));
            }
            for l in &r.loops {
                if l.len() > self.segments.len() || self.loop_points(l).is_none() {
                    return Err(VectorError::Invalid("петля области не замкнута"));
                }
            }
            if let Some(fills) = &r.fills {
                if fills.len() > MAX_REGION_FILLS {
                    return Err(VectorError::TooLarge);
                }
                if fills
                    .iter()
                    .any(|f| f.kind != FillKind::Solid || !(0.0..=1.0).contains(&f.opacity))
                {
                    return Err(VectorError::Invalid("заливка области"));
                }
            }
        }
        Ok(())
    }

    /// Двоичная форма для документа и кэша сцены.
    pub fn encode(&self) -> Vec<u8> {
        let mut w = Writer::new();
        w.u8(FORMAT);
        w.u32(self.points.len() as u32);
        for p in &self.points {
            w.f64(p.x);
            w.f64(p.y);
            w.u8(HANDLE_MODES
                .iter()
                .position(|m| *m == p.handle_mode)
                .unwrap_or(0) as u8);
            w.f64(p.corner_radius);
        }
        w.u32(self.segments.len() as u32);
        for s in &self.segments {
            w.u32(s.start);
            w.u32(s.end);
            for v in s.tangent_start.iter().chain(&s.tangent_end) {
                w.f64(*v);
            }
        }
        w.u32(self.regions.len() as u32);
        for r in &self.regions {
            w.u8(match r.rule {
                FillRule::NonZero => 0,
                FillRule::EvenOdd => 1,
            });
            w.u32(r.loops.len() as u32);
            for l in &r.loops {
                w.u32(l.len() as u32);
                for i in l {
                    w.u32(*i);
                }
            }
            match &r.fills {
                None => w.u8(0),
                Some(fills) => {
                    w.u8(1);
                    w.u8(fills.len().min(u8::MAX as usize) as u8);
                    for f in fills.iter().take(u8::MAX as usize) {
                        // Хэша у заливки области нет: строка пустая.
                        let _ = write_fill(&mut w, f);
                    }
                }
            }
        }
        w.buf
    }

    /// Сеть из двоичной формы — не доверенной: длины сверяются с остатком байтов
    /// до выделения памяти, затем `validate`.
    pub fn decode(bytes: &[u8]) -> Result<VectorNetwork, VectorError> {
        let bad = VectorError::Invalid("двоичная форма");
        let mut r = Reader::new(bytes);
        if r.u8() != Some(FORMAT) {
            return Err(VectorError::Invalid("версия формы"));
        }
        let left = |r: &Reader, per: usize| r.remaining() / per;
        let n = r.u32().ok_or(bad.clone())? as usize;
        if n > MAX_VECTOR_POINTS {
            return Err(VectorError::TooManyPoints);
        }
        if n > left(&r, 25) {
            return Err(bad);
        }
        let mut points = Vec::with_capacity(n);
        for _ in 0..n {
            let (x, y) = (r.f64().ok_or(bad.clone())?, r.f64().ok_or(bad.clone())?);
            let handle_mode = *HANDLE_MODES
                .get(r.u8().ok_or(bad.clone())? as usize)
                .ok_or(bad.clone())?;
            let corner_radius = r.f64().ok_or(bad.clone())?;
            points.push(VectorPoint {
                x,
                y,
                handle_mode,
                corner_radius,
            });
        }
        let n = r.u32().ok_or(bad.clone())? as usize;
        if n > MAX_VECTOR_SEGMENTS {
            return Err(VectorError::TooLarge);
        }
        if n > left(&r, 40) {
            return Err(bad);
        }
        let mut segments = Vec::with_capacity(n);
        for _ in 0..n {
            let (start, end) = (r.u32().ok_or(bad.clone())?, r.u32().ok_or(bad.clone())?);
            let mut t = [0.0; 4];
            for v in &mut t {
                *v = r.f64().ok_or(bad.clone())?;
            }
            segments.push(VectorSegment {
                start,
                end,
                tangent_start: [t[0], t[1]],
                tangent_end: [t[2], t[3]],
            });
        }
        let n = r.u32().ok_or(bad.clone())? as usize;
        if n > MAX_VECTOR_LOOPS || n > left(&r, 6) {
            return Err(VectorError::TooLarge);
        }
        let mut regions = Vec::with_capacity(n);
        let mut loops_total = 0usize;
        for _ in 0..n {
            let rule = match r.u8().ok_or(bad.clone())? {
                0 => FillRule::NonZero,
                1 => FillRule::EvenOdd,
                _ => return Err(bad),
            };
            let k = r.u32().ok_or(bad.clone())? as usize;
            loops_total += k;
            if loops_total > MAX_VECTOR_LOOPS || k > left(&r, 4) {
                return Err(VectorError::TooLarge);
            }
            let mut loops = Vec::with_capacity(k);
            for _ in 0..k {
                let len = r.u32().ok_or(bad.clone())? as usize;
                if len > MAX_VECTOR_SEGMENTS || len > left(&r, 4) {
                    return Err(VectorError::TooLarge);
                }
                let mut l = Vec::with_capacity(len);
                for _ in 0..len {
                    l.push(r.u32().ok_or(bad.clone())?);
                }
                loops.push(l);
            }
            let fills = match r.u8().ok_or(bad.clone())? {
                0 => None,
                1 => {
                    let m = r.u8().ok_or(bad.clone())? as usize;
                    let mut fills = Vec::with_capacity(m);
                    for _ in 0..m {
                        fills.push(read_fill(&mut r).ok_or(bad.clone())?);
                    }
                    Some(fills)
                }
                _ => return Err(bad),
            };
            regions.push(VectorRegion { rule, loops, fills });
        }
        if r.remaining() != 0 {
            return Err(bad);
        }
        let net = VectorNetwork {
            points,
            segments,
            regions,
        };
        net.validate()?;
        Ok(net)
    }

    /// Строка для буфера обмена (base64 двоичной формы).
    pub fn to_base64(&self) -> String {
        base64_encode(&self.encode())
    }

    /// Сеть из строки буфера обмена (не доверенной).
    pub fn from_base64(s: &str) -> Result<VectorNetwork, VectorError> {
        let bytes = base64_decode(s).ok_or(VectorError::Invalid("base64"))?;
        VectorNetwork::decode(&bytes)
    }
}

const B64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

fn base64_encode(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b = [
            chunk[0],
            *chunk.get(1).unwrap_or(&0),
            *chunk.get(2).unwrap_or(&0),
        ];
        let n = (b[0] as u32) << 16 | (b[1] as u32) << 8 | b[2] as u32;
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(B64[(n >> (18 - 6 * i) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

fn base64_decode(s: &str) -> Option<Vec<u8>> {
    let s = s.as_bytes();
    if !s.len().is_multiple_of(4) {
        return None;
    }
    let mut out = Vec::with_capacity(s.len() / 4 * 3);
    for chunk in s.chunks(4) {
        let mut n = 0u32;
        let mut pad = 0;
        for (i, &c) in chunk.iter().enumerate() {
            let v = match c {
                b'=' if i >= 2 => {
                    pad += 1;
                    0
                }
                _ if pad > 0 => return None,
                _ => B64.iter().position(|x| *x == c)? as u32,
            };
            n = n << 6 | v;
        }
        let b = [(n >> 16) as u8, (n >> 8) as u8, n as u8];
        out.extend_from_slice(&b[..3 - pad]);
    }
    Some(out)
}

/// Окончание открытого конца обводки (как `strokeCap` в Figma).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StrokeCap {
    None,
    Round,
    Square,
    ArrowLines,
    ArrowTriangle,
    Circle,
    Diamond,
}

impl StrokeCap {
    pub const ALL: [StrokeCap; 7] = [
        StrokeCap::None,
        StrokeCap::Round,
        StrokeCap::Square,
        StrokeCap::ArrowLines,
        StrokeCap::ArrowTriangle,
        StrokeCap::Circle,
        StrokeCap::Diamond,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            StrokeCap::None => "none",
            StrokeCap::Round => "round",
            StrokeCap::Square => "square",
            StrokeCap::ArrowLines => "arrowLines",
            StrokeCap::ArrowTriangle => "arrowTriangle",
            StrokeCap::Circle => "circle",
            StrokeCap::Diamond => "diamond",
        }
    }

    pub fn parse(s: &str) -> Option<StrokeCap> {
        StrokeCap::ALL.into_iter().find(|c| c.as_str() == s)
    }
}

/// Соединение сегментов обводки.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StrokeJoin {
    Miter,
    Round,
    Bevel,
}

impl StrokeJoin {
    pub const ALL: [StrokeJoin; 3] = [StrokeJoin::Miter, StrokeJoin::Round, StrokeJoin::Bevel];

    pub fn as_str(self) -> &'static str {
        match self {
            StrokeJoin::Miter => "miter",
            StrokeJoin::Round => "round",
            StrokeJoin::Bevel => "bevel",
        }
    }

    pub fn parse(s: &str) -> Option<StrokeJoin> {
        StrokeJoin::ALL.into_iter().find(|c| c.as_str() == s)
    }
}

/// Предел острого угла по умолчанию (как у `kurbo::Stroke` и SVG).
pub const DEFAULT_MITER_LIMIT: f64 = 4.0;
/// Допустимый предел острого угла (как в Figma: угол от ~0,01° до 180°).
pub const MITER_LIMIT_RANGE: std::ops::RangeInclusive<f64> = 1.0..=100.0;

/// Стиль обводки узла: окончания начала и конца открытых путей, соединение и
/// предел острого угла. Как в Figma, общий для всех обводок узла.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StrokeStyle {
    pub cap_start: StrokeCap,
    pub cap_end: StrokeCap,
    pub join: StrokeJoin,
    pub miter_limit: f64,
}

impl Default for StrokeStyle {
    fn default() -> Self {
        StrokeStyle {
            cap_start: StrokeCap::None,
            cap_end: StrokeCap::None,
            join: StrokeJoin::Miter,
            miter_limit: DEFAULT_MITER_LIMIT,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_round_trips_every_tail() {
        for n in 0..10u8 {
            let bytes: Vec<u8> = (0..n).map(|i| i.wrapping_mul(37)).collect();
            assert_eq!(base64_decode(&base64_encode(&bytes)), Some(bytes));
        }
        assert_eq!(base64_encode(b"Man"), "TWFu");
        assert_eq!(base64_decode("TQ=="), Some(b"M".to_vec()));
        assert_eq!(base64_decode("T=Q="), None);
        assert_eq!(base64_decode("TQ="), None);
    }
}
