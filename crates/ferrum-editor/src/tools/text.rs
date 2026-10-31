//! Инструмент «Текст» (T, спека подсистемы 3, раздел 7): щелчок — новый текст
//! «авто по ширине» с курсором, протяжка — текст фиксированного размера по рамке.
//! Текст встаёт в самый глубокий фрейм под точкой начала. В документ он попадёт
//! с первым символом: до того — черновик правки (`Editor::start_draft_text`).

use std::collections::HashSet;

use ferrum_doc::{NodeId, NodeKind, Props, TextSizing};
use kurbo::{Affine, Point, Rect};

use super::{Ctx, Cursor, Flow, Tool, ToolKind, ToolOverlay, DRAG_THRESHOLD_PX};
use crate::input::{Modifiers, PointerEvent};
use crate::selection::drop_frame_at;

/// Рамка меньше стольких пикселей мира по любой оси — это щелчок.
const MIN_FIXED: f64 = 1.0;

#[derive(Debug, Clone, Copy)]
struct Start {
    screen: Point,
    world: Point,
    parent: Option<NodeId>,
    parent_world: Affine,
}

#[derive(Debug, Clone, Copy)]
enum State {
    Idle,
    Pressed(Start),
    Drawing(Start, Rect),
}

pub struct TextTool {
    state: State,
}

impl Default for TextTool {
    fn default() -> Self {
        TextTool::new()
    }
}

impl TextTool {
    pub fn new() -> TextTool {
        TextTool { state: State::Idle }
    }

    fn snap(cx: &Ctx, p: Point) -> Point {
        if cx.settings.pixel_snap {
            Point::new(p.x.round(), p.y.round())
        } else {
            p
        }
    }

    /// Свойства нового текста: левый верхний угол (в осях родителя) и размер
    /// рамки; без рамки — «авто по ширине».
    fn props(start: &Start, rect: Option<Rect>) -> Props {
        let mut p = Props::defaults(NodeKind::Text);
        let origin = rect.map_or(start.world, |r| r.origin());
        let local = start.parent_world.inverse() * origin;
        (p.x, p.y) = (local.x, local.y);
        if let (Some(r), Some(t)) = (rect, p.text.as_mut()) {
            let t = std::sync::Arc::make_mut(t);
            t.props.sizing = TextSizing::Fixed;
            (p.w, p.h) = (r.width(), r.height());
        }
        p
    }
}

impl Tool for TextTool {
    fn kind(&self) -> ToolKind {
        ToolKind::Text
    }

    fn pointer_down(&mut self, cx: &mut Ctx, e: &PointerEvent) {
        let world = Self::snap(cx, cx.to_world(e.point));
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
        });
    }

    fn pointer_move(&mut self, cx: &mut Ctx, e: &PointerEvent) {
        let start = match self.state {
            State::Idle => return,
            State::Pressed(s) => {
                if (e.point - s.screen).hypot() < DRAG_THRESHOLD_PX {
                    return;
                }
                s
            }
            State::Drawing(s, _) => s,
        };
        let p = Self::snap(cx, cx.to_world(e.point));
        self.state = State::Drawing(start, Rect::from_points(start.world, p));
    }

    fn pointer_up(&mut self, cx: &mut Ctx, e: &PointerEvent) -> Flow {
        self.pointer_move(cx, e);
        let state = std::mem::replace(&mut self.state, State::Idle);
        let (start, rect) = match state {
            State::Idle => return Flow::Stay,
            State::Pressed(s) => (s, None),
            State::Drawing(s, r) => (
                s,
                (r.width() >= MIN_FIXED && r.height() >= MIN_FIXED).then_some(r),
            ),
        };
        cx.out.new_text = Some((start.parent, Self::props(&start, rect)));
        Flow::Finished
    }

    fn key(&mut self, _cx: &mut Ctx, _modifiers: Modifiers) {}

    fn cancel(&mut self, _cx: &mut Ctx) {
        self.state = State::Idle;
    }

    fn is_busy(&self) -> bool {
        !matches!(self.state, State::Idle)
    }

    fn cursor(&self) -> Cursor {
        Cursor::Text
    }

    fn overlay(&self) -> ToolOverlay {
        ToolOverlay {
            marquee: match self.state {
                State::Drawing(_, r) => Some(r),
                _ => None,
            },
            ..Default::default()
        }
    }
}
