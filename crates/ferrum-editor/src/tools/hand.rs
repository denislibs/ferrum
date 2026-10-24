//! Рука (H, удержание Space, средняя кнопка): панорамирование холста. Документ не
//! меняет, поэтому и отменять нечего.

use kurbo::Point;

use super::{Ctx, Cursor, Flow, Tool, ToolKind, ToolOverlay};
use crate::input::{Modifiers, PointerEvent};

#[derive(Default)]
pub struct HandTool {
    last: Option<Point>,
}

impl HandTool {
    pub fn new() -> HandTool {
        HandTool::default()
    }
}

impl Tool for HandTool {
    fn kind(&self) -> ToolKind {
        ToolKind::Hand
    }

    fn pointer_down(&mut self, cx: &mut Ctx, e: &PointerEvent) {
        *cx.hover = None;
        self.last = Some(e.point);
    }

    fn pointer_move(&mut self, cx: &mut Ctx, e: &PointerEvent) {
        if let Some(last) = self.last {
            cx.camera.pan_by(e.point - last);
            self.last = Some(e.point);
        }
    }

    fn pointer_up(&mut self, cx: &mut Ctx, e: &PointerEvent) -> Flow {
        self.pointer_move(cx, e);
        self.last = None;
        Flow::Stay
    }

    fn key(&mut self, _: &mut Ctx, _: Modifiers) {}

    fn cancel(&mut self, _: &mut Ctx) {
        self.last = None;
    }

    fn is_busy(&self) -> bool {
        self.last.is_some()
    }

    fn cursor(&self) -> Cursor {
        if self.last.is_some() {
            Cursor::Grabbing
        } else {
            Cursor::Grab
        }
    }

    fn overlay(&self) -> ToolOverlay {
        ToolOverlay::default()
    }
}
