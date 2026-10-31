//! Режим правки вектора, инструмент выделения (спека подсистемы 3, раздел 9):
//! точки, ручки и сегменты щелчком (Shift — добавить и убрать) и рамкой,
//! перемещение с привязкой к точкам сети и пиксельной сетке, ручки с
//! зеркальностью (Alt — разорвать), изгиб сегмента (Ctrl/Cmd + протяжка),
//! удаление с сохранением формы (Delete) и с разрывом (Shift+Delete), сдвиг
//! стрелками, «Заливка области» (B). Каждая правка — один шаг отмены.

use std::collections::BTreeSet;

use ferrum_doc::NodeId;
use kurbo::{Line, Point, Rect, Vec2};

use crate::commands::CommandId;
use crate::editor::{Editor, EditorEvent};
use crate::input::PointerEvent;
use crate::snap::SNAP_THRESHOLD_PX;
use crate::tools::{sync, DRAG_THRESHOLD_PX};
use crate::vector_edit::{point_of, Drag, VectorSelection, POINT_HIT_PX};
use crate::vector_ops::{
    bend, delete_points_break, delete_points_keep_shape, delete_segments, move_points, set_handle,
    toggle_fill, HandleEnd,
};

impl Editor {
    /// Убирает из выделения номера за пределами сети: после пера, удаления или
    /// отмены чужой номер не должен дойти до операций сети.
    fn clamp_vector_selection(&mut self) {
        if let Some(v) = &mut self.vector_edit {
            v.sel.clamp(&v.work);
        }
    }

    /// Видимая ручка под экранной точкой: у выделенных точек, концов выделенных
    /// сегментов и выделенных ручек (те же, что рисует оверлей).
    fn vector_handle_at(&self, screen: Point) -> Option<(usize, HandleEnd)> {
        let overlay = self.vector_overlay()?;
        let v = self.vector_edit.as_ref()?;
        let to_screen = self.camera.world_to_screen_transform() * v.world;
        overlay.handles.iter().find_map(|&(s, at_start)| {
            let seg = v.work.segments.get(s as usize)?;
            let (p, t) = if at_start {
                (seg.start, seg.tangent_start)
            } else {
                (seg.end, seg.tangent_end)
            };
            let h = point_of(&v.work, p) + Vec2::new(t[0], t[1]);
            ((to_screen * h - screen).hypot() <= POINT_HIT_PX).then_some((
                s as usize,
                if at_start {
                    HandleEnd::Start
                } else {
                    HandleEnd::End
                },
            ))
        })
    }

    /// Нажатие инструментом выделения в правке. `false` — нажатие по другому
    /// узлу: правка кончилась, нажатие — обычное.
    pub(crate) fn vector_select_down(&mut self, e: &PointerEvent) -> bool {
        self.clamp_vector_selection();
        let world = self.camera.screen_to_world(e.point);
        let primary = e.modifiers.primary(self.settings.platform);
        let shift = e.modifiers.shift;
        if let Some((seg, end)) = self.vector_handle_at(e.point) {
            let v = self.vector_edit.as_mut().expect("правка идёт");
            v.sel.handles = BTreeSet::from([(seg as u32, end)]);
            v.drag = Some(Drag::Handle {
                seg,
                end,
                origin: v.work.clone(),
            });
            return true;
        }
        if let Some(p) = self.vector_point_at(e.point) {
            let v = self.vector_edit.as_mut().expect("правка идёт");
            let anchor = v.to_world(point_of(&v.work, p));
            if shift && v.sel.points.contains(&p) {
                v.sel.points.remove(&p);
                return true;
            }
            if shift {
                v.sel.points.insert(p);
            } else if !v.sel.points.contains(&p) {
                v.sel = VectorSelection {
                    points: BTreeSet::from([p]),
                    ..Default::default()
                };
            }
            v.sel.handles.clear();
            v.drag = Some(Drag::Points {
                start_screen: e.point,
                start_world: world,
                anchor,
                origin: v.work.clone(),
                moved: false,
            });
            return true;
        }
        if let Some((seg, t)) = self.vector_segment_at(e.point) {
            let v = self.vector_edit.as_mut().expect("правка идёт");
            if primary {
                v.drag = Some(Drag::Bend {
                    seg,
                    t,
                    start_world: world,
                    origin: v.work.clone(),
                });
                return true;
            }
            let s = seg as u32;
            if shift && v.sel.segments.contains(&s) {
                v.sel.segments.remove(&s);
                return true;
            }
            if shift {
                v.sel.segments.insert(s);
            } else if !v.sel.segments.contains(&s) {
                v.sel = VectorSelection {
                    segments: BTreeSet::from([s]),
                    ..Default::default()
                };
            }
            v.sel.handles.clear();
            v.drag = Some(Drag::Points {
                start_screen: e.point,
                start_world: world,
                anchor: world,
                origin: v.work.clone(),
                moved: false,
            });
            return true;
        }
        // Другой узел под точкой — выход из правки, щелчок как обычный.
        let tol = crate::tools::HIT_TOLERANCE_PX / self.camera.zoom();
        let edited = self.vector_edit_node();
        if let Some(hit) = self.scene.hit_test(world, tol) {
            if Some(hit) != edited && !self.scene.is_draft(hit) {
                self.end_vector_edit();
                self.selection.set(Vec::new());
                return false;
            }
        }
        let v = self.vector_edit.as_mut().expect("правка идёт");
        let base = if shift {
            v.sel.clone()
        } else {
            VectorSelection::default()
        };
        v.sel = base.clone();
        v.drag = Some(Drag::Marquee {
            start_world: world,
            base,
            rect: None,
        });
        true
    }

    /// Протяжка инструментом выделения в правке.
    pub(crate) fn vector_select_move(&mut self, e: &PointerEvent) {
        let world = self.camera.screen_to_world(e.point);
        let zoom = self.camera.zoom();
        let pixel_snap = self.settings.pixel_snap;
        let snapping = self.settings.snapping;
        let Some(v) = self.vector_edit.as_mut() else {
            return;
        };
        let Some(drag) = v.drag.clone() else {
            return;
        };
        match drag {
            Drag::Points {
                start_screen,
                start_world,
                anchor,
                origin,
                moved,
            } => {
                if !moved && (e.point - start_screen).hypot() < DRAG_THRESHOLD_PX {
                    return;
                }
                let moving = v.sel.moving_points(&origin);
                let mut target = anchor + (world - start_world);
                if e.modifiers.shift {
                    // Shift — по оси, вдоль которой сдвиг больше.
                    let d = target - anchor;
                    target = if d.x.abs() >= d.y.abs() {
                        Point::new(target.x, anchor.y)
                    } else {
                        Point::new(anchor.x, target.y)
                    };
                }
                let mut lines = Vec::new();
                let (mut sx, mut sy) = (false, false);
                if snapping {
                    // Привязка к точкам сети, которые не двигаются: по каждой оси —
                    // ближайшая в пределах порога.
                    let moving_set: BTreeSet<u32> = moving.iter().copied().collect();
                    let threshold = SNAP_THRESHOLD_PX / zoom;
                    let mut bx: Option<(f64, Point)> = None;
                    let mut by: Option<(f64, Point)> = None;
                    for i in 0..origin.points.len() as u32 {
                        if moving_set.contains(&i) {
                            continue;
                        }
                        let q = v.to_world(point_of(&origin, i));
                        let (dx, dy) = ((q.x - target.x).abs(), (q.y - target.y).abs());
                        if dx <= threshold && bx.is_none_or(|b| dx < b.0) {
                            bx = Some((dx, q));
                        }
                        if dy <= threshold && by.is_none_or(|b| dy < b.0) {
                            by = Some((dy, q));
                        }
                    }
                    if let Some((_, q)) = bx {
                        target.x = q.x;
                        sx = true;
                        lines.push(Line::new((q.x, q.y), (q.x, target.y)));
                    }
                    if let Some((_, q)) = by {
                        target.y = q.y;
                        sy = true;
                        lines.push(Line::new((q.x, q.y), (target.x, q.y)));
                    }
                }
                // Пиксельная сетка — по осям без привязки к точке.
                if pixel_snap {
                    if !sx {
                        target.x = target.x.round();
                    }
                    if !sy {
                        target.y = target.y.round();
                    }
                }
                let d = v.to_shape(target) - v.to_shape(anchor);
                v.work = move_points(&origin, &moving, d);
                v.snap_lines = lines;
                v.drag = Some(Drag::Points {
                    start_screen,
                    start_world,
                    anchor,
                    origin,
                    moved: true,
                });
            }
            Drag::Handle { seg, end, origin } => {
                let s = &origin.segments[seg];
                let p = point_of(
                    &origin,
                    if end == HandleEnd::Start {
                        s.start
                    } else {
                        s.end
                    },
                );
                let t = v.to_shape(world) - p;
                v.work = set_handle(&origin, seg, end, t, e.modifiers.alt);
            }
            Drag::Bend {
                seg,
                t,
                start_world,
                origin,
            } => {
                let d = v.to_shape(world) - v.to_shape(start_world);
                v.work = bend(&origin, seg, t, d);
            }
            Drag::Marquee {
                start_world, base, ..
            } => {
                let rect = Rect::from_points(start_world, world);
                let mut sel = base.clone();
                for (i, p) in v.work.points.iter().enumerate() {
                    if rect.contains(v.to_world(Point::new(p.x, p.y))) {
                        sel.points.insert(i as u32);
                    }
                }
                v.sel = sel;
                v.drag = Some(Drag::Marquee {
                    start_world,
                    base,
                    rect: Some(rect),
                });
                return;
            }
            Drag::PenHandle { .. } => return,
        }
        self.show_vector_work();
    }

    /// Отпускание инструментом выделения: правка жеста — одним шагом.
    pub(crate) fn vector_select_up(&mut self, _e: &PointerEvent) {
        let Some(v) = self.vector_edit.as_mut() else {
            return;
        };
        let drag = v.drag.take();
        v.snap_lines.clear();
        let changed = match &drag {
            Some(Drag::Points { origin, .. })
            | Some(Drag::Handle { origin, .. })
            | Some(Drag::Bend { origin, .. }) => *origin != v.work,
            _ => false,
        };
        if changed {
            self.commit_vector_work();
        } else if matches!(
            drag,
            Some(Drag::Points { .. }) | Some(Drag::Handle { .. }) | Some(Drag::Bend { .. })
        ) {
            self.scene.clear_preview();
        }
    }

    /// «Заливка области» в точке нажатия: заливка грани включается или снимается.
    pub(crate) fn bucket_click(&mut self, e: &PointerEvent) {
        let world = self.camera.screen_to_world(e.point);
        let Some(v) = self.vector_edit.as_mut() else {
            return;
        };
        let filled = v.base.fills.iter().any(|f| f.visible);
        let at = v.to_shape(world);
        if let Some(net) = toggle_fill(&v.work, at, filled) {
            v.work = net;
            self.commit_vector_work();
        }
    }

    /// Удаление выделенного в правке: точки с сохранением формы (или с
    /// разрывом), иначе сегменты. Пустая сеть — узел удаляется (как в Figma).
    pub(crate) fn vector_delete(&mut self, break_path: bool) {
        self.clamp_vector_selection();
        let Some(v) = self.vector_edit.as_mut() else {
            return;
        };
        if v.sel.is_empty() || v.id.is_none() {
            return;
        }
        let net = if !v.sel.points.is_empty() {
            let pts: Vec<u32> = v.sel.points.iter().copied().collect();
            if break_path {
                delete_points_break(&v.work, &pts)
            } else {
                delete_points_keep_shape(&v.work, &pts)
            }
        } else if !v.sel.segments.is_empty() {
            let segs: Vec<u32> = v.sel.segments.iter().copied().collect();
            delete_segments(&v.work, &segs)
        } else {
            return;
        };
        v.sel = VectorSelection::default();
        if net.points.is_empty() {
            let id = v.id.expect("проверено выше");
            self.vector_edit = None;
            self.delete_node(id);
            return;
        }
        v.work = net;
        self.commit_vector_work();
    }

    /// Удаляет узел одним шагом (сеть правки опустела).
    fn delete_node(&mut self, id: NodeId) {
        self.scene.clear_preview();
        let page = self.page;
        let result = self.doc.transact(page, vec![id], |tx| {
            tx.delete_nodes(&[id])?;
            Ok(vec![])
        });
        let changed = sync(&mut self.doc, page, &mut self.scene, &mut self.out);
        match result {
            Ok(_) => {
                if changed {
                    self.out.steps += 1;
                }
                self.selection.clear();
            }
            Err(e) => self.out.events.push(EditorEvent::Error(e.to_string())),
        }
    }

    /// Сдвиг выделенных точек стрелками на `d` в мире.
    fn vector_nudge(&mut self, d: Vec2) {
        self.clamp_vector_selection();
        let Some(v) = self.vector_edit.as_mut() else {
            return;
        };
        let moving = v.sel.moving_points(&v.work);
        if moving.is_empty() {
            return;
        }
        let local = v.vec_to_shape(d);
        v.work = move_points(&v.work, &moving, local);
        self.commit_vector_work();
    }

    /// Команда раскладки во время правки вектора. `true` — правка её выполнила;
    /// `false` — команда обычная (вид, отмена и повтор работают в правке, а
    /// правки выделения узлов сначала заканчивают её).
    pub(crate) fn vector_command(&mut self, cmd: CommandId) -> bool {
        use CommandId as C;
        match cmd {
            C::Delete => self.vector_delete(false),
            C::SelectAll => {
                if let Some(v) = &mut self.vector_edit {
                    v.sel = VectorSelection {
                        points: (0..v.work.points.len() as u32).collect(),
                        ..Default::default()
                    };
                }
            }
            C::Escape => self.vector_escape(),
            C::NudgeLeft => self.vector_nudge(Vec2::new(-1.0, 0.0)),
            C::NudgeRight => self.vector_nudge(Vec2::new(1.0, 0.0)),
            C::NudgeUp => self.vector_nudge(Vec2::new(0.0, -1.0)),
            C::NudgeDown => self.vector_nudge(Vec2::new(0.0, 1.0)),
            C::NudgeLeftBig => self.vector_nudge(Vec2::new(-10.0, 0.0)),
            C::NudgeRightBig => self.vector_nudge(Vec2::new(10.0, 0.0)),
            C::NudgeUpBig => self.vector_nudge(Vec2::new(0.0, -10.0)),
            C::NudgeDownBig => self.vector_nudge(Vec2::new(0.0, 10.0)),
            // Вид, отмена, инструменты пера и выделения — как обычно.
            C::Undo
            | C::Redo
            | C::ToolPen
            | C::ToolMove
            | C::HandHold
            | C::ToolHand
            | C::ZoomToFit
            | C::ZoomToSelection
            | C::Zoom100
            | C::ZoomIn
            | C::ZoomOut
            | C::ToggleUi
            | C::Save => return false,
            // Остальное — правки узлов: правка вектора кончается.
            _ => {
                self.end_vector_edit();
                return false;
            }
        }
        self.emit();
        true
    }
}
