//! Инструмент «Масштаб» (K, спека подсистемы 3, раздел 11): ресайз выделения,
//! который масштабирует и свойства — толщину обводок, радиусы скругления, размер
//! текста, интерлиньяж и трекинг в пикселях, отступы абзацев, радиусы точек
//! вектора (окончания обводки и стрелки строятся от её толщины и растут вместе с
//! ней). Масштаб всегда равномерный — как в Figma: у обводки и текста одно число,
//! растянуть их по одной оси нельзя. Содержимое групп, булевых групп и фреймов
//! масштабируется вместе с ними.
//!
//! Жест холста и поле «Масштаб» панели считают правки одной функцией
//! `scale_patches`: превью и запись совпадают, запись — одна транзакция.

use std::collections::HashMap;
use std::sync::Arc;

use ferrum_doc::{LetterSpacing, LineHeight, NodeId, Props, PropsPatch, Stroke, TextSizing};
use ferrum_scene::shape::COORD_LIMIT;
use ferrum_text::Fonts;
use kurbo::{Affine, Point};

use crate::geometry::{scale_patches_deep, Base, SelectionBox};
use crate::text::fit_patch;

/// Пределы масштаба поля панели: 1 % … 10 000 %.
pub const MIN_SCALE_FACTOR: f64 = 0.01;
pub const MAX_SCALE_FACTOR: f64 = 100.0;
/// Пределы стиля текста (те же, что у секции «Текст»: размер 1–10 000 px,
/// интерлиньяж 0–10 000 px, трекинг и отступ ±10 000 px, интервал 0–10 000 px).
const TEXT_SIZE_MIN: f64 = 1.0;
const TEXT_LIMIT: f64 = 10_000.0;

/// Якорь масштаба из панели: доли рамки выделения по осям (0, 0,5 или 1), как
/// сетка 3×3 у инструмента Scale в Figma.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Anchor {
    fx: f64,
    fy: f64,
}

impl Anchor {
    pub const CENTER: Anchor = Anchor { fx: 0.5, fy: 0.5 };
    pub const TOP_LEFT: Anchor = Anchor { fx: 0.0, fy: 0.0 };

    /// Только девять точек сетки; остальное — `None` (недоверенный ввод).
    pub fn new(fx: f64, fy: f64) -> Option<Anchor> {
        let ok = |v: f64| v == 0.0 || v == 0.5 || v == 1.0;
        (ok(fx) && ok(fy)).then_some(Anchor { fx, fy })
    }

    pub fn fx(self) -> f64 {
        self.fx
    }

    pub fn fy(self) -> f64 {
        self.fy
    }
}

/// Равномерный коэффициент жеста `t`, заданного в рамке `frame` (её матрица в
/// мир), с рамкой `rect`: модуль масштаба по оси, у которой рамка не нулевая
/// (у горизонтальной линии высоты нет — масштаб по ширине).
pub fn uniform_factor(t: Affine, frame: Affine, width: f64, height: f64) -> f64 {
    if frame.determinant().abs() < 1e-24 {
        return 1.0;
    }
    let g = (frame.inverse() * t * frame).as_coeffs();
    let k = if width > 0.0 {
        g[0].abs()
    } else if height > 0.0 {
        g[3].abs()
    } else {
        1.0
    };
    if k.is_finite() {
        k
    } else {
        1.0
    }
}

/// Матрица масштаба `k` вокруг якоря рамки выделения (в мире). `flip` — отражение
/// единственного узла выделения (его `flip_x`/`flip_y`): рамка узла отражена
/// вместе с ним, а сетка 3×3 панели — по видимой картинке, поэтому доли якоря по
/// отражённой оси зеркалятся. Поворот остаётся в рамке (решение 4 плана).
pub fn anchor_transform(sbox: &SelectionBox, anchor: Anchor, k: f64, flip: (bool, bool)) -> Affine {
    let r = sbox.rect;
    let fx = if flip.0 { 1.0 - anchor.fx } else { anchor.fx };
    let fy = if flip.1 { 1.0 - anchor.fy } else { anchor.fy };
    let p = Point::new(r.x0 + fx * r.width(), r.y0 + fy * r.height());
    let local = Affine::translate(p.to_vec2()) * Affine::scale(k) * Affine::translate(-p.to_vec2());
    sbox.to_world * local * sbox.to_world.inverse()
}

fn clamp_text(v: f64, lo: f64) -> f64 {
    v.clamp(lo, TEXT_LIMIT)
}

fn scale_line_height(v: LineHeight, k: f64) -> LineHeight {
    match v {
        LineHeight::Px(px) => LineHeight::Px(clamp_text(px * k, 0.0)),
        other => other,
    }
}

fn scale_letter_spacing(v: LetterSpacing, k: f64) -> LetterSpacing {
    match v {
        LetterSpacing::Px(px) => LetterSpacing::Px(clamp_text(px * k, -TEXT_LIMIT)),
        other => other,
    }
}

/// Добавляет в `patch` масштабированные на `k` свойства узла `p`: обводки,
/// радиус, стиль и разметку текста, радиусы точек вектора. Нечего менять — патч
/// не трогается.
pub fn scale_props(p: &Props, k: f64, patch: &mut PropsPatch) {
    if !(k.is_finite() && k > 0.0) || (k - 1.0).abs() < 1e-12 {
        return;
    }
    if !p.strokes.is_empty() {
        patch.strokes = Some(
            p.strokes
                .iter()
                .map(|s| Stroke {
                    width: (s.width * k).clamp(0.0, COORD_LIMIT),
                    ..s.clone()
                })
                .collect(),
        );
    }
    if p.corner_radius > 0.0 {
        patch.corner_radius = Some((p.corner_radius * k).clamp(0.0, COORD_LIMIT));
    }
    if let Some(t) = &p.text {
        let mut props = t.props.clone();
        let s = &mut props.style;
        s.size = clamp_text(s.size * k, TEXT_SIZE_MIN);
        s.line_height = scale_line_height(s.line_height, k);
        s.letter_spacing = scale_letter_spacing(s.letter_spacing, k);
        props.paragraph_spacing = clamp_text(props.paragraph_spacing * k, 0.0);
        props.paragraph_indent = clamp_text(props.paragraph_indent * k, -TEXT_LIMIT);
        patch.text = Some(props);
        // Разметка участков: только то, что задано в пикселях.
        let styled = t.content.runs.iter().any(|r| {
            r.attrs.size.is_some()
                || matches!(r.attrs.line_height, Some(LineHeight::Px(_)))
                || matches!(r.attrs.letter_spacing, Some(LetterSpacing::Px(_)))
        });
        if styled {
            let mut content = t.content.clone();
            for r in &mut content.runs {
                let a = &mut r.attrs;
                a.size = a.size.map(|v| clamp_text(v * k, TEXT_SIZE_MIN));
                a.line_height = a.line_height.map(|v| scale_line_height(v, k));
                a.letter_spacing = a.letter_spacing.map(|v| scale_letter_spacing(v, k));
            }
            patch.content = Some(content);
        }
    }
    if let Some(net) = &p.vector {
        if net.points.iter().any(|pt| pt.corner_radius > 0.0) {
            let mut net = (**net).clone();
            for pt in &mut net.points {
                pt.corner_radius = (pt.corner_radius * k).clamp(0.0, COORD_LIMIT);
            }
            patch.vector = Some(Arc::new(net));
        }
    }
}

/// Правки инструмента «Масштаб»: положение и размер (`scale_patches_deep`) и
/// масштабированные свойства каждого узла поддерева. `k` — равномерный
/// коэффициент жеста. С `fonts` у текстов «авто» размер рамки берётся по
/// раскладке с новым кеглем (как после правки текста; шрифт ещё грузится —
/// остаётся масштабированная рамка).
pub fn scale_patches(
    bases: &[Base],
    t: Affine,
    frame: Affine,
    k: f64,
    fonts: Option<&mut Fonts>,
) -> Vec<(NodeId, PropsPatch)> {
    let mut out = scale_patches_deep(bases, t, frame);
    let mut at: HashMap<NodeId, usize> = out
        .iter()
        .enumerate()
        .map(|(i, (id, _))| (*id, i))
        .collect();
    for b in bases {
        let mut extra = PropsPatch::default();
        scale_props(&b.props, k, &mut extra);
        if extra == PropsPatch::default() {
            continue;
        }
        match at.get(&b.id) {
            Some(&i) => merge(&mut out[i].1, extra),
            None => {
                at.insert(b.id, out.len());
                out.push((b.id, extra));
            }
        }
    }
    keep_fixed_text_in_proportion(bases, k, &mut out);
    if let Some(fonts) = fonts {
        let by_id: HashMap<NodeId, &Props> = bases.iter().map(|b| (b.id, &b.props)).collect();
        for (id, patch) in &mut out {
            let Some(before) = by_id.get(id) else {
                continue;
            };
            let auto = before
                .text_props()
                .is_some_and(|t| t.sizing != TextSizing::Fixed);
            if !auto {
                continue;
            }
            let mut p = (*before).clone();
            patch.apply_to(&mut p);
            if let Ok(fit) = fit_patch(fonts, &p, &p) {
                patch.w = fit.w.or(patch.w);
                patch.h = fit.h.or(patch.h);
                patch.x = fit.x.or(patch.x);
                patch.y = fit.y.or(patch.y);
            }
        }
    }
    out
}

/// Кегль не меньше 1 px, а рамка «Фикс.» текста при малом масштабе продолжала бы
/// сжиматься: текст и рамка разошлись бы. Рамка берёт коэффициент кегля после
/// зажима, центр остаётся там, куда его привёл масштаб.
fn keep_fixed_text_in_proportion(bases: &[Base], k: f64, out: &mut [(NodeId, PropsPatch)]) {
    for b in bases {
        let Some(t) = b.props.text_props() else {
            continue;
        };
        let size = t.style.size;
        if t.sizing != TextSizing::Fixed
            || size <= 0.0
            || size.is_nan()
            || size * k >= TEXT_SIZE_MIN
        {
            continue;
        }
        let k_eff = TEXT_SIZE_MIN / size;
        let Some((_, patch)) = out.iter_mut().find(|(id, _)| *id == b.id) else {
            continue;
        };
        let (Some(x), Some(y), Some(w), Some(h)) = (patch.x, patch.y, patch.w, patch.h) else {
            continue;
        };
        let (nw, nh) = (b.props.w * k_eff, b.props.h * k_eff);
        patch.x = Some(x + (w - nw) / 2.0);
        patch.y = Some(y + (h - nh) / 2.0);
        patch.w = Some(nw);
        patch.h = Some(nh);
    }
}

/// Поля `extra` поверх `patch` (только масштабируемые свойства).
fn merge(patch: &mut PropsPatch, extra: PropsPatch) {
    if extra.strokes.is_some() {
        patch.strokes = extra.strokes;
    }
    if extra.corner_radius.is_some() {
        patch.corner_radius = extra.corner_radius;
    }
    if extra.text.is_some() {
        patch.text = extra.text;
    }
    if extra.content.is_some() {
        patch.content = extra.content;
    }
    if extra.vector.is_some() {
        patch.vector = extra.vector;
    }
}
