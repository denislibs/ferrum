//! Рисование фигур: фрейм (F, A), прямоугольник (R), эллипс (O), линия (L),
//! стрелка (Shift+L — линия со стрелкой на конце, как в Figma) и
//! размещение изображения (Ctrl/Cmd+Shift+K). Пока кнопка нажата, фигура — черновик
//! сцены; при отпускании она создаётся одной транзакцией и выделяется, а редактор
//! возвращается к «Перемещению», как в Figma. Shift — квадрат, круг или линия с
//! шагом 45°; Alt — от центра. Фигура встаёт в самый глубокий фрейм под точкой
//! начала.

use std::collections::HashSet;

use ferrum_doc::{Fill, NodeId, NodeKind, Props, StrokeCap};
use ferrum_scene::Draft;
use kurbo::{Affine, Point, Rect, Vec2};

use super::{Ctx, Cursor, Flow, Tool, ToolKind, ToolOverlay, DRAG_THRESHOLD_PX};
use crate::clipboard::full_patch;
use crate::geometry::{decompose, decompose_with, snap_angle};
use crate::input::{Modifiers, PointerEvent};
use crate::selection::drop_frame_at;
use crate::snap::{Snapper, SNAP_THRESHOLD_PX};

/// Размер фигуры, созданной кликом без перетаскивания.
pub const CLICK_SIZE: f64 = 100.0;
/// Фигура меньше пикселя по любой оси (линия — короче пикселя) считается кликом:
/// дотянутая обратно к точке начала, она получает размер клика, как в Figma, а не
/// становится невидимым узлом 0 × 0.
const MIN_DRAWN_PX: f64 = 1.0;

/// Что рисуем.
#[derive(Debug, Clone, PartialEq)]
pub enum Shape {
    Frame,
    Rect,
    Ellipse,
    Line,
    /// Линия с окончанием `arrowLines` на конце.
    Arrow,
    /// Изображение по хэшу байтов (байты кладёт в документ интерфейс) и его
    /// исходный размер: клик ставит его в этом размере, перетаскивание сохраняет
    /// пропорции.
    Image {
        hash: String,
        width: f64,
        height: f64,
    },
}

impl Shape {
    fn kind(&self) -> NodeKind {
        match self {
            Shape::Frame => NodeKind::Frame,
            Shape::Rect => NodeKind::Rect,
            Shape::Ellipse => NodeKind::Ellipse,
            Shape::Line | Shape::Arrow => NodeKind::Line,
            Shape::Image { .. } => NodeKind::Image,
        }
    }
}

#[derive(Debug, Clone)]
struct Start {
    screen: Point,
    world: Point,
    parent: Option<NodeId>,
    parent_world: Affine,
    snapper: Snapper,
}

#[derive(Debug, Clone)]
enum State {
    Idle,
    Pressed(Start),
    Drawing {
        start: Start,
        draft: NodeId,
        props: Props,
    },
}

pub struct DrawTool {
    shape: Shape,
    state: State,
    lines: Vec<kurbo::Line>,
    last: Option<PointerEvent>,
}

impl DrawTool {
    pub fn new(shape: Shape) -> DrawTool {
        DrawTool {
            shape,
            state: State::Idle,
            lines: Vec::new(),
            last: None,
        }
    }

    pub fn shape(&self) -> &Shape {
        &self.shape
    }

    /// Свойства фигуры от точки `a` до точки `b` (в мире).
    fn props_between(&self, start: &Start, a: Point, b: Point, mods: Modifiers) -> Props {
        let kind = self.shape.kind();
        let mut props = Props::defaults(kind);
        if let Shape::Image { hash, .. } = &self.shape {
            props.fills = vec![Fill::image(hash.clone())];
        }
        if self.shape == Shape::Arrow {
            props.stroke_style.cap_end = StrokeCap::ArrowLines;
        }
        let inv = start.parent_world.inverse();
        if kind == NodeKind::Line {
            let mut v = b - a;
            if mods.shift {
                let ang = snap_angle(v.atan2().to_degrees(), 45.0).to_radians();
                v = Vec2::new(ang.cos(), ang.sin()) * v.hypot();
            }
            let from = if mods.alt { a - v } else { a };
            let len = if mods.alt { 2.0 * v.hypot() } else { v.hypot() };
            let world = Affine::translate(from.to_vec2()) * Affine::rotate(v.atan2());
            let pose = decompose(inv * world, len, 0.0);
            props.x = pose.x;
            props.y = pose.y;
            props.w = pose.w;
            props.rotation = pose.rotation;
            (props.flip_x, props.flip_y) = (pose.flip_x, pose.flip_y);
            return props;
        }
        let mut v = b - a;
        let aspect = match &self.shape {
            Shape::Image { width, height, .. } if *width > 0.0 && *height > 0.0 => {
                Some(width / height)
            }
            _ if mods.shift => Some(1.0),
            _ => None,
        };
        if let Some(ratio) = aspect {
            // Сторона по большему смещению, знак — в сторону курсора.
            let w = v.x.abs().max(v.y.abs() * ratio);
            let h = w / ratio;
            v = Vec2::new(w.copysign(v.x), h.copysign(v.y));
        }
        let rect = if mods.alt {
            Rect::from_points(a - v, a + v)
        } else {
            Rect::from_points(a, a + v)
        };
        // Размер нужен разложению: в отражённом родителе фигура получает своё
        // отражение относительно центра рамки и на экране выглядит неотражённой.
        let pose = decompose_with(
            inv * Affine::translate(rect.origin().to_vec2()),
            rect.width(),
            rect.height(),
            0.0,
        );
        props.x = pose.x;
        props.y = pose.y;
        props.rotation = pose.rotation;
        (props.flip_x, props.flip_y) = (pose.flip_x, pose.flip_y);
        props.w = rect.width();
        props.h = rect.height();
        props
    }

    /// Свойства фигуры, созданной кликом: стандартный размер от точки клика.
    fn click_props(&self, start: &Start) -> Props {
        let (w, h) = match &self.shape {
            Shape::Image { width, height, .. } => (*width, *height),
            Shape::Line | Shape::Arrow => (CLICK_SIZE, 0.0),
            _ => (CLICK_SIZE, CLICK_SIZE),
        };
        let b = start.world + Vec2::new(w, h);
        let mut p = self.props_between(start, start.world, b, Modifiers::NONE);
        if self.shape.kind() == NodeKind::Line {
            return p;
        }
        p.w = w;
        p.h = h;
        p
    }

    fn create(&self, cx: &mut Ctx, parent: Option<NodeId>, props: Props) {
        let kind = props.kind;
        cx.commit(|tx, _| {
            let id = tx.create_node(parent, None, kind)?;
            tx.set_props(id, &full_patch(&props))?;
            Ok(vec![id])
        });
    }

    fn update(&mut self, cx: &mut Ctx, e: &PointerEvent) {
        let State::Drawing { start, draft, .. } = &self.state else {
            return;
        };
        let (start, draft) = (start.clone(), *draft);
        let mut p = cx.to_world(e.point);
        self.lines.clear();
        if cx.settings.snapping {
            let threshold = SNAP_THRESHOLD_PX / cx.camera.zoom();
            let s = start.snapper.snap(
                cx.scene,
                &[p.x],
                &[p.y],
                Rect::from_points(start.world, p),
                threshold,
                cx.view(),
            );
            p.x += s.dx.unwrap_or(0.0);
            p.y += s.dy.unwrap_or(0.0);
            self.lines = s.lines;
        }
        if cx.settings.pixel_snap {
            p = Point::new(p.x.round(), p.y.round());
        }
        let props = self.props_between(&start, start.world, p, e.modifiers);
        cx.scene.set_drafts(&[Draft {
            id: draft,
            parent: start.parent,
            props: props.clone(),
        }]);
        self.state = State::Drawing {
            start,
            draft,
            props,
        };
    }
}

impl Tool for DrawTool {
    fn kind(&self) -> ToolKind {
        match self.shape {
            Shape::Frame => ToolKind::Frame,
            Shape::Rect => ToolKind::Rect,
            Shape::Ellipse => ToolKind::Ellipse,
            Shape::Line => ToolKind::Line,
            Shape::Arrow => ToolKind::Arrow,
            Shape::Image { .. } => ToolKind::Image,
        }
    }

    fn pointer_down(&mut self, cx: &mut Ctx, e: &PointerEvent) {
        self.last = Some(*e);
        let mut world = cx.to_world(e.point);
        if cx.settings.pixel_snap {
            world = Point::new(world.x.round(), world.y.round());
        }
        let parent = drop_frame_at(cx.scene, world, cx.tolerance(), &HashSet::new());
        let parent_world = parent
            .and_then(|p| cx.scene.node(p))
            .map_or(Affine::IDENTITY, |n| n.world);
        *cx.hover = None;
        self.state = State::Pressed(Start {
            screen: e.point,
            world,
            parent,
            parent_world,
            snapper: Snapper::for_parent(cx.scene, parent, &[]),
        });
    }

    fn pointer_move(&mut self, cx: &mut Ctx, e: &PointerEvent) {
        self.last = Some(*e);
        if let State::Pressed(start) = &self.state {
            if (e.point - start.screen).hypot() < DRAG_THRESHOLD_PX {
                return;
            }
            let start = start.clone();
            let draft = cx.draft_id();
            let props = Props::defaults(self.shape.kind());
            self.state = State::Drawing {
                start,
                draft,
                props,
            };
        }
        self.update(cx, e);
    }

    fn pointer_up(&mut self, cx: &mut Ctx, e: &PointerEvent) -> Flow {
        self.last = Some(*e);
        self.update(cx, e);
        let state = std::mem::replace(&mut self.state, State::Idle);
        self.lines.clear();
        match state {
            State::Idle => Flow::Stay,
            State::Pressed(start) => {
                let props = self.click_props(&start);
                self.create(cx, start.parent, props);
                Flow::Finished
            }
            State::Drawing { start, props, .. } => {
                let tiny = if props.kind == NodeKind::Line {
                    props.w < MIN_DRAWN_PX
                } else {
                    props.w < MIN_DRAWN_PX || props.h < MIN_DRAWN_PX
                };
                let props = if tiny {
                    self.click_props(&start)
                } else {
                    props
                };
                self.create(cx, start.parent, props);
                Flow::Finished
            }
        }
    }

    fn key(&mut self, cx: &mut Ctx, modifiers: Modifiers) {
        if let Some(mut last) = self.last {
            last.modifiers = modifiers;
            self.last = Some(last);
            self.update(cx, &last);
        }
    }

    fn cancel(&mut self, cx: &mut Ctx) {
        self.state = State::Idle;
        self.lines.clear();
        cx.scene.clear_preview();
    }

    fn is_busy(&self) -> bool {
        !matches!(self.state, State::Idle)
    }

    fn cursor(&self) -> Cursor {
        Cursor::Crosshair
    }

    fn overlay(&self) -> ToolOverlay {
        ToolOverlay {
            snap_lines: self.lines.clone(),
            selection: match &self.state {
                State::Drawing { draft, .. } => Some(vec![*draft]),
                _ => None,
            },
            ..Default::default()
        }
    }
}
