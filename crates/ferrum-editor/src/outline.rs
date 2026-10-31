//! «Контур обводки» (Ctrl/Cmd+Alt+O, спека подсистемы 3, раздел 10): обводки
//! выделенных слоёв становятся вектором, чья заливка — краска обводки, одним
//! шагом отмены.
//!
//! Вектор встаёт прямо над слоем, с его именем и позой; у каждой видимой
//! обводки — своя область своего цвета (верхняя обводка — верхняя область).
//! Слой с видимыми заливками остаётся без обводок, без заливок — удаляется (как
//! в Figma, где от такого слоя остаётся только контур). Обводка по центру
//! сохраняет кривые; внутри и снаружи — обрезка контуром формы булевой операцией
//! (ломаные с точностью `BOOLEAN_TOLERANCE`).

use ferrum_doc::{
    BooleanOp, Fill, FillKind, FillRule, NodeId, NodeKind, Props, Stroke, StrokeAlign,
};
use ferrum_scene::boolean::{combine, flatten_path, normalize, BOOLEAN_TOLERANCE};
use ferrum_scene::vector::{self as vpath, StrokeClip};
use ferrum_scene::Slot;
use kurbo::{Affine, BezPath, Join};

use crate::editor::{Editor, EditorEvent};
use crate::selection::locked;
use crate::vector_edit::as_vector;
use crate::vector_ops::{merge_networks, path_network, reframe};

/// Видимая обводка: ненулевая толщина и не прозрачный цвет.
fn visible(s: &Stroke) -> bool {
    s.width > 0.0 && s.color.a > 0
}

/// Обрезка контура `outline` формой `shape` (оба — пути в координатах формы).
fn clip(outline: &BezPath, shape: &BezPath, inside: bool) -> BezPath {
    let o = normalize(flatten_path(outline, BOOLEAN_TOLERANCE), FillRule::NonZero);
    let s = normalize(flatten_path(shape, BOOLEAN_TOLERANCE), FillRule::NonZero);
    let op = if inside {
        BooleanOp::Intersect
    } else {
        BooleanOp::Subtract
    };
    combine(op, &[o, s]).to_path()
}

impl Editor {
    /// Контур одной обводки узла `slot` в координатах его формы и правило заливки.
    fn stroke_outline(&self, slot: Slot, s: &Stroke) -> Option<(BezPath, FillRule)> {
        let n = self.scene.at(slot);
        let p = &n.props;
        if p.kind == NodeKind::Text {
            // Глифы (без подчёркивания и зачёркивания — рендер их не обводит): по центру — как есть, внутри и снаружи — двойной толщиной с
            // обрезкой по глифам (как рисует рендер).
            let glyphs = {
                let mut fonts = self.fonts.try_borrow_mut().ok()?;
                ferrum_render::text_glyphs_outline(&mut fonts, p)?
            };
            let w = match s.align {
                StrokeAlign::Center => s.width,
                _ => 2.0 * s.width,
            };
            let k = kurbo::Stroke::new(w).with_join(Join::Miter);
            let out = kurbo::stroke(
                glyphs.iter(),
                &k,
                &kurbo::StrokeOpts::default(),
                ferrum_scene::shape::PATH_TOLERANCE,
            );
            return Some(match s.align {
                StrokeAlign::Center => (out, FillRule::NonZero),
                a => (
                    clip(&out, &glyphs, a == StrokeAlign::Inside),
                    FillRule::NonZero,
                ),
            });
        }
        if let Some(ps) = n.path_shape() {
            let out = vpath::stroke_outline(ps, s, &p.stroke_style);
            return Some(match vpath::stroke_clip(ps, s) {
                StrokeClip::None => (out, FillRule::NonZero),
                c => (
                    clip(&out, &ps.stroke, c == StrokeClip::Inside),
                    FillRule::NonZero,
                ),
            });
        }
        // Кольцо фигуры (прямоугольник, эллипс, линия) — уже с положением.
        n.geometry().stroke_ring(s).map(|r| (r, FillRule::EvenOdd))
    }

    /// Обводки выделения — в векторы (одна команда — один шаг).
    pub(crate) fn outline_stroke(&mut self) {
        self.finish_vector_edit();
        let ids = self.ordered_selection();
        let mut jobs: Vec<(NodeId, Props, bool)> = Vec::new();
        let mut loading = false;
        // Пропущено из-за блокировки или скрытия.
        let mut skipped = false;
        for id in ids {
            let Some(slot) = self.scene.slot(id) else {
                continue;
            };
            if locked(&self.scene, slot) || !self.scene.is_visible(slot) {
                skipped = true;
                continue;
            }
            let p = self.scene.at(slot).props.clone();
            // Своей обводки по форме нет у группы; обводка фрейма — рамка
            // монтажной области, её контур не строится (как в Figma).
            if matches!(p.kind, NodeKind::Group | NodeKind::Frame) {
                continue;
            }
            let mut parts = Vec::new();
            for s in p.strokes.iter().filter(|s| visible(s)) {
                let Some((path, rule)) = self.stroke_outline(slot, s) else {
                    loading |= p.kind == NodeKind::Text;
                    continue;
                };
                let mut net = path_network(&path, rule);
                for r in &mut net.regions {
                    r.fills = Some(vec![Fill::solid(s.color)]);
                }
                if !net.regions.is_empty() {
                    parts.push((net, Affine::IDENTITY));
                }
            }
            if parts.is_empty() {
                continue;
            }
            let mut base = p.clone();
            as_vector(&mut base);
            base.fills = Vec::new();
            base.strokes = Vec::new();
            base.stroke_style = Default::default();
            let patch = reframe(&base, merge_networks(&parts));
            patch.apply_to(&mut base);
            let keep = p.fills.iter().any(|f| {
                f.visible && f.opacity > 0.0 && (f.kind != FillKind::Solid || f.color.a > 0)
            });
            jobs.push((id, base, keep));
        }
        if jobs.is_empty() {
            let msg = if loading {
                "Шрифты ещё загружаются — контур обводки текста не построен"
            } else if skipped {
                "Заблокированные и скрытые слои не обрабатываются"
            } else {
                "У выделенных слоёв нет видимых обводок"
            };
            self.out.events.push(EditorEvent::Error(msg.into()));
            return;
        }
        // Часть выделения обработана, часть — нет: сказать и об этом.
        if loading {
            self.out.events.push(EditorEvent::Error(
                "Шрифты ещё загружаются — контур обводки текста не построен".into(),
            ));
        }
        if skipped {
            self.out.events.push(EditorEvent::Error(
                "Заблокированные и скрытые слои не обрабатываются".into(),
            ));
        }
        self.ctx().commit(|tx, _| {
            let mut created = Vec::new();
            for (id, props, keep) in &jobs {
                let parent = tx.parent(*id);
                let index = tx
                    .children(parent)
                    .iter()
                    .position(|c| c == id)
                    .map_or(usize::MAX, |i| i + 1);
                created.push(tx.create_with_props(parent, Some(index), props)?);
                if *keep {
                    tx.set_props(
                        *id,
                        &ferrum_doc::PropsPatch {
                            strokes: Some(Vec::new()),
                            ..Default::default()
                        },
                    )?;
                } else {
                    tx.delete_nodes(&[*id])?;
                }
            }
            Ok(created)
        });
    }
}
