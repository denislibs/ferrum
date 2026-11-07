//! Метки комментариев на холсте (спека подсистемы 4, раздел 10.2): капля с инициалами автора
//! цветом автора, счётчик ответов и отметка «не прочитано», открытая метка обведена цветом
//! выделения, закрытое обсуждение — серое; метки, слипшиеся на экране, — круг с числом. Где метки
//! стоят и какие слиплись, решает редактор (раскладка по сцене и камере); здесь — только как они
//! выглядят. Метки рисуются только на экране: превью и экспорт оверлеев не рисуют вовсе.
//!
//! Каждая часть метки (капля, группа, плашка счётчика) — готовая картинка, как чужой курсор
//! (`overlay::CursorSprites`): глифы инициалов и обводка у 1000 меток каждый кадр стоили бы
//! больше всей цели кадра. Картинок строится не больше `PIN_BUILDS_PER_FRAME` за кадр, хранится не
//! больше `PIN_SPRITE_LIMIT`: остальные метки кадра — простой заливкой формы без текста (дёшево),
//! следующие кадры — картинкой.

use std::collections::HashMap;
use std::sync::Arc;

use kurbo::{
    Affine, BezPath, Circle, Point, Rect, RoundedRect, RoundedRectRadii, Shape, Stroke, Vec2,
};
use vello_cpu::peniko::{Extend, Fill};
use vello_cpu::Pixmap;

use crate::backend::{CpuBackend, ImageKey, ImagePaint, RenderBackend, Rgba};
use crate::text::TextEngine;

/// Сторона капли метки — CSS-пиксели. Остриё капли (левый нижний угол) — точка метки.
pub const PIN_SIZE: f64 = 28.0;
/// Диаметр круга группы меток — CSS-пиксели; центр круга — точка группы.
pub const PIN_CLUSTER_SIZE: f64 = 32.0;
/// Столько картинок меток строится за кадр (около 0,1 мс каждая).
pub const PIN_BUILDS_PER_FRAME: usize = 8;
/// Столько картинок меток хранится: авторов на странице — десятки, у каждого 3 вида капли
/// (обычная, закрытая, открытая), плашек счётчика — до 200 (0…100 ответов, с непрочитанным и
/// без). Сверх предела метка рисуется простой заливкой, пока не освободится место.
pub const PIN_SPRITE_LIMIT: usize = 256;
/// Счётчик ответов больше этого показывается как «99+».
pub const PIN_REPLIES_SHOWN: u32 = 99;

/// Метка комментария или группа слипшихся меток в мировых координатах.
#[derive(Debug, Clone, PartialEq)]
pub struct PinMark {
    /// У метки — остриё капли, у группы — центр круга.
    pub at: Point,
    pub kind: PinKind,
}

#[derive(Debug, Clone, PartialEq)]
pub enum PinKind {
    /// Обсуждение: инициалы и цвет автора, ответов, не прочитано, закрыто, открыто сейчас.
    Pin {
        initials: Arc<str>,
        color: Rgba,
        replies: u32,
        unread: bool,
        resolved: bool,
        open: bool,
    },
    /// Слипшиеся метки: сколько и есть ли среди них непрочитанные.
    Cluster { count: u32, unread: bool },
    /// Метка нового комментария, пока его пишут: капля цвета выделения без текста.
    Draft,
}

/// Попадает ли экранная точка `p` в метку с экранной точкой `anchor` (CSS-пиксели): капля —
/// квадрат над остриём, группа — круг. Одна геометрия у рисования и у попадания.
pub fn pin_contains(kind: &PinKind, anchor: Point, p: Point) -> bool {
    match kind {
        PinKind::Cluster { .. } => p.distance(anchor) <= PIN_CLUSTER_SIZE / 2.0,
        PinKind::Pin { .. } | PinKind::Draft => {
            let d = p - anchor;
            (0.0..=PIN_SIZE).contains(&d.x) && (-PIN_SIZE..=0.0).contains(&d.y)
        }
    }
}

/// Часть метки — одна картинка. Цвета — байтами (ключ кэша).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum Part {
    Bubble {
        initials: Arc<str>,
        color: [u8; 4],
        resolved: bool,
        /// Открыта сейчас: кольцо цвета выделения.
        open: Option<[u8; 4]>,
    },
    Draft {
        color: [u8; 4],
    },
    Cluster {
        /// Не больше `PIN_REPLIES_SHOWN + 1` («99+»).
        count: u32,
    },
    /// Плашка в правом верхнем углу: число ответов (цвет выделения — есть непрочитанное),
    /// без ответов — точка «не прочитано».
    Badge {
        count: u32,
        unread: bool,
        accent: [u8; 4],
    },
}

/// Серый закрытого обсуждения и тёмный круг группы — нейтральные цвета дизайн-системы.
const RESOLVED: Rgba = Rgba::new([
    0x8f as f32 / 255.0,
    0x8a as f32 / 255.0,
    0x84 as f32 / 255.0,
    1.0,
]);
const CLUSTER: Rgba = Rgba::new([
    0x2c as f32 / 255.0,
    0x2a as f32 / 255.0,
    0x28 as f32 / 255.0,
    1.0,
]);
const BADGE: Rgba = Rgba::new([
    0x3a as f32 / 255.0,
    0x37 as f32 / 255.0,
    0x35 as f32 / 255.0,
    1.0,
]);
const WHITE: Rgba = Rgba::new([1.0, 1.0, 1.0, 1.0]);

fn rgba(c: [u8; 4]) -> Rgba {
    Rgba::from_rgba8(c[0], c[1], c[2], c[3])
}

/// Текст на заливке: тёмный на светлом, иначе белый (как у подписи чужого курсора).
fn ink_on(c: Rgba) -> Rgba {
    let [r, g, b, _] = c.components;
    if 0.2126 * r + 0.7152 * g + 0.0722 * b > 0.6 {
        Rgba::from_rgba8(0x1a, 0x19, 0x18, 255)
    } else {
        WHITE
    }
}

fn count_text(n: u32) -> String {
    if n > PIN_REPLIES_SHOWN {
        format!("{PIN_REPLIES_SHOWN}+")
    } else {
        n.to_string()
    }
}

/// Части метки и где их начала относительно точки метки (пиксели устройства).
fn parts(kind: &PinKind, accent: [u8; 4], dpr: f64) -> [Option<(Part, Vec2)>; 2] {
    let cap = |n: u32| n.min(PIN_REPLIES_SHOWN + 1);
    match kind {
        PinKind::Pin {
            initials,
            color,
            replies,
            unread,
            resolved,
            open,
        } => {
            let body = Part::Bubble {
                initials: initials.clone(),
                color: color.to_rgba8().to_u8_array(),
                resolved: *resolved,
                // Цвет кольца нужен ключу только у открытой: у закрытых он не рисуется.
                open: open.then_some(accent),
            };
            let badge = (*replies > 0 || *unread).then(|| {
                (
                    Part::Badge {
                        count: cap(*replies),
                        unread: *unread,
                        accent,
                    },
                    Vec2::new((PIN_SIZE - 3.0) * dpr, -(PIN_SIZE - 3.0) * dpr),
                )
            });
            [Some((body, Vec2::ZERO)), badge]
        }
        PinKind::Cluster { count, unread } => {
            let r = PIN_CLUSTER_SIZE / 2.0 * 0.72 * dpr;
            let badge = unread.then(|| {
                (
                    Part::Badge {
                        count: 0,
                        unread: true,
                        accent,
                    },
                    Vec2::new(r, -r),
                )
            });
            [
                Some((Part::Cluster { count: cap(*count) }, Vec2::ZERO)),
                badge,
            ]
        }
        PinKind::Draft => [Some((Part::Draft { color: accent }, Vec2::ZERO)), None],
    }
}

/// Капля: квадрат над остриём `o` со скруглёнными углами, кроме левого нижнего.
fn bubble_path(o: Point, dpr: f64) -> BezPath {
    let s = PIN_SIZE * dpr;
    RoundedRect::from_rect(
        Rect::new(o.x, o.y - s, o.x + s, o.y),
        RoundedRectRadii::new(s / 2.0, s / 2.0, s / 2.0, 0.0),
    )
    .to_path(0.1)
}

/// Плашка счётчика с центром в `o`: её прямоугольник и подпись.
fn badge_shape(
    text: &mut TextEngine,
    count: u32,
    o: Point,
    dpr: f64,
) -> (BezPath, Option<Arc<crate::text::Label>>) {
    if count == 0 {
        return (Circle::new(o, 4.5 * dpr).to_path(0.1), None);
    }
    let label = text.label(&count_text(count), 9.0 * dpr, 600, 40.0 * dpr);
    let h = 14.0 * dpr;
    let w = label.as_ref().map_or(h, |l| (l.width + 8.0 * dpr).max(h));
    let r = Rect::from_center_size(o, (w, h));
    (RoundedRect::from_rect(r, h / 2.0).to_path(0.1), label)
}

/// Рисует часть с началом в `o` (пиксели устройства).
fn paint(backend: &mut impl RenderBackend, text: &mut TextEngine, part: &Part, o: Point, dpr: f64) {
    backend.set_transform(Affine::IDENTITY);
    match part {
        Part::Bubble {
            initials,
            color,
            resolved,
            open,
        } => {
            let path = bubble_path(o, dpr);
            let fill = if *resolved { RESOLVED } else { rgba(*color) };
            // Открытая метка — широкое кольцо цвета выделения под белым (как выбранная в Figma).
            // Обводка — до заливки: внутренняя половина штриха под ней.
            if let Some(ring) = open {
                backend.set_color(rgba(*ring));
                backend.stroke_path(&path, &Stroke::new(8.0 * dpr));
            }
            backend.set_color(WHITE);
            backend.stroke_path(&path, &Stroke::new(3.0 * dpr));
            backend.set_color(fill);
            backend.fill_path(&path, Fill::NonZero);
            let s = PIN_SIZE * dpr;
            if let Some(l) = text.label(initials, 11.0 * dpr, 600, s - 6.0 * dpr) {
                let base = o.y - s / 2.0 + (l.ascent - l.descent) / 2.0;
                l.draw(
                    backend,
                    Point::new(o.x + (s - l.width) / 2.0, base),
                    ink_on(fill),
                );
            }
        }
        Part::Draft { color } => {
            let path = bubble_path(o, dpr);
            backend.set_color(WHITE);
            backend.stroke_path(&path, &Stroke::new(3.0 * dpr));
            backend.set_color(rgba(*color));
            backend.fill_path(&path, Fill::NonZero);
        }
        Part::Cluster { count } => {
            let c = Circle::new(o, PIN_CLUSTER_SIZE / 2.0 * dpr).to_path(0.1);
            backend.set_color(WHITE);
            backend.stroke_path(&c, &Stroke::new(3.0 * dpr));
            backend.set_color(CLUSTER);
            backend.fill_path(&c, Fill::NonZero);
            if let Some(l) =
                text.label(&count_text(*count), 12.0 * dpr, 600, PIN_CLUSTER_SIZE * dpr)
            {
                let base = o.y + (l.ascent - l.descent) / 2.0;
                l.draw(backend, Point::new(o.x - l.width / 2.0, base), WHITE);
            }
        }
        Part::Badge {
            count,
            unread,
            accent,
        } => {
            let (path, label) = badge_shape(text, *count, o, dpr);
            let fill = if *unread { rgba(*accent) } else { BADGE };
            backend.set_transform(Affine::IDENTITY);
            backend.set_color(WHITE);
            backend.stroke_path(&path, &Stroke::new(3.0 * dpr));
            backend.set_color(fill);
            backend.fill_path(&path, Fill::NonZero);
            if let Some(l) = label {
                let base = o.y + (l.ascent - l.descent) / 2.0;
                l.draw(backend, Point::new(o.x - l.width / 2.0, base), ink_on(fill));
            }
        }
    }
}

/// Дешёвая замена картинки, пока её нет (предел построений за кадр или кэш полон): форма
/// одной заливкой, без обводки и текста. Плашки нет.
fn paint_plain(backend: &mut impl RenderBackend, part: &Part, o: Point, dpr: f64) {
    backend.set_transform(Affine::IDENTITY);
    match part {
        Part::Bubble {
            color, resolved, ..
        } => {
            backend.set_color(if *resolved { RESOLVED } else { rgba(*color) });
            backend.fill_path(&bubble_path(o, dpr), Fill::NonZero);
        }
        Part::Draft { color } => {
            backend.set_color(rgba(*color));
            backend.fill_path(&bubble_path(o, dpr), Fill::NonZero);
        }
        Part::Cluster { .. } => {
            backend.set_color(CLUSTER);
            backend.fill_path(
                &Circle::new(o, PIN_CLUSTER_SIZE / 2.0 * dpr).to_path(0.1),
                Fill::NonZero,
            );
        }
        Part::Badge { .. } => {}
    }
}

/// Границы части относительно её начала (пиксели устройства), с полем под обводку.
fn part_bounds(text: &mut TextEngine, part: &Part, dpr: f64) -> Rect {
    let m = 4.0 * dpr;
    match part {
        Part::Bubble { open: Some(_), .. } => {
            let s = PIN_SIZE * dpr;
            Rect::new(0.0, -s, s, 0.0).inflate(m + 2.0 * dpr, m + 2.0 * dpr)
        }
        Part::Bubble { .. } | Part::Draft { .. } => {
            let s = PIN_SIZE * dpr;
            Rect::new(0.0, -s, s, 0.0).inflate(m, m)
        }
        Part::Cluster { .. } => {
            let r = PIN_CLUSTER_SIZE / 2.0 * dpr;
            Rect::new(-r, -r, r, r).inflate(m, m)
        }
        Part::Badge { count, .. } => badge_shape(text, *count, Point::ZERO, dpr)
            .0
            .bounding_box()
            .inflate(m, m),
    }
}

#[derive(Clone)]
struct Sprite {
    key: ImageKey,
    pixmap: Arc<Pixmap>,
    /// Где в картинке начало части, пиксели устройства (целые).
    origin: Vec2,
    used: u64,
}

fn build(text: &mut TextEngine, part: &Part, dpr: f64) -> Sprite {
    let b = part_bounds(text, part, dpr);
    let origin = Vec2::new(-b.x0.floor(), -b.y0.floor());
    let (w, h) = (
        (b.x1 + origin.x).ceil().max(1.0) as u16,
        (b.y1 + origin.y).ceil().max(1.0) as u16,
    );
    let mut cpu = CpuBackend::new(w, h);
    paint(&mut cpu, text, part, origin.to_point(), dpr);
    Sprite {
        key: ImageKey::next_raster(),
        pixmap: Arc::new(cpu.render()),
        origin,
        used: 0,
    }
}

/// Картинки частей меток по виду и плотности пикселей.
#[derive(Default)]
pub(crate) struct PinSprites {
    map: HashMap<(Part, u32), Sprite>,
    frame: u64,
}

impl PinSprites {
    pub(crate) fn len(&self) -> usize {
        self.map.len()
    }

    fn get(&mut self, key: &(Part, u32)) -> Option<Sprite> {
        let frame = self.frame;
        self.map.get_mut(key).map(|s| {
            s.used = frame;
            s.clone()
        })
    }

    /// Есть ли место для новой картинки: давно не нужные уходят первыми, нужные в этом или
    /// прошлом кадре остаются — тогда сверх предела новая не строится.
    fn room(&mut self) -> bool {
        if self.map.len() >= PIN_SPRITE_LIMIT {
            let frame = self.frame;
            self.map.retain(|_, s| s.used + 1 >= frame);
        }
        self.map.len() < PIN_SPRITE_LIMIT
    }
}

/// Метки комментариев в порядке списка (последняя — сверху). Метка за краем экрана (с плашкой)
/// не рисуется и картинку не строит.
pub(crate) fn draw_pins(
    backend: &mut impl RenderBackend,
    text: &mut TextEngine,
    sprites: &mut PinSprites,
    view: Affine,
    dpr: f64,
    accent: Rgba,
    marks: &[PinMark],
) {
    if marks.is_empty() {
        return;
    }
    sprites.frame += 1;
    let accent = accent.to_rgba8().to_u8_array();
    let images = backend.supports_images();
    let (w, h) = backend.size();
    let reach = (PIN_SIZE + PIN_CLUSTER_SIZE) * dpr;
    let screen = Rect::new(0.0, 0.0, w as f64, h as f64).inflate(reach, reach);
    let scale = (dpr * 64.0).round() as u32;
    let mut builds = 0;
    for m in marks {
        let at = view * m.at;
        if !(at.x.is_finite() && at.y.is_finite() && screen.contains(at)) {
            continue;
        }
        // Картинка ложится пиксель в пиксель: точка метки — на целом пикселе.
        let at = Point::new(at.x.round(), at.y.round());
        for (part, offset) in parts(&m.kind, accent, dpr).into_iter().flatten() {
            let o = at + Vec2::new(offset.x.round(), offset.y.round());
            if !images {
                paint(backend, text, &part, o, dpr);
                continue;
            }
            let key = (part, scale);
            let mut sprite = sprites.get(&key);
            if sprite.is_none() && builds < PIN_BUILDS_PER_FRAME && sprites.room() {
                builds += 1;
                let s = build(text, &key.0, dpr);
                sprites.map.insert(
                    key.clone(),
                    Sprite {
                        used: sprites.frame,
                        ..s.clone()
                    },
                );
                sprite = Some(s);
            }
            match sprite {
                Some(s) => {
                    let origin = o - s.origin;
                    let (sw, sh) = (s.pixmap.width() as f64, s.pixmap.height() as f64);
                    backend.set_transform(Affine::IDENTITY);
                    backend.set_image(&ImagePaint {
                        key: s.key,
                        pixmap: s.pixmap,
                        transform: Affine::translate(origin.to_vec2()),
                        extend: Extend::Pad,
                        alpha: 1.0,
                    });
                    backend.fill_rect(&Rect::new(origin.x, origin.y, origin.x + sw, origin.y + sh));
                }
                None => paint_plain(backend, &key.0, o, dpr),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reply_count_over_99_reads_99_plus() {
        assert_eq!(count_text(7), "7");
        assert_eq!(count_text(99), "99");
        assert_eq!(count_text(100), "99+");
        assert_eq!(count_text(u32::MAX), "99+");
    }
}
