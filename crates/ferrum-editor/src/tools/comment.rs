//! «Комментарий» (C, спека подсистемы 4, раздел 10.3): режим комментариев. Нажатия в нём
//! целиком у меток (`Editor::comment_press`: открыть, перетащить, поставить новую), поэтому сам
//! инструмент ничего не делает — пока он выбран, остальные инструменты не работают, наведения
//! и рамки выделения нет. Камера работает как обычно: колесо, щипок, Space и средняя кнопка,
//! клавиши зума — их редактор обрабатывает мимо инструмента.

use super::{Ctx, Cursor, Flow, Tool, ToolKind, ToolOverlay};
use crate::input::{Modifiers, PointerEvent};

#[derive(Default)]
pub struct CommentTool;

impl Tool for CommentTool {
    fn kind(&self) -> ToolKind {
        ToolKind::Comment
    }

    fn pointer_down(&mut self, _: &mut Ctx, _: &PointerEvent) {}

    fn pointer_move(&mut self, cx: &mut Ctx, _: &PointerEvent) {
        *cx.hover = None;
    }

    fn pointer_up(&mut self, _: &mut Ctx, _: &PointerEvent) -> Flow {
        Flow::Stay
    }

    fn key(&mut self, _: &mut Ctx, _: Modifiers) {}

    fn cancel(&mut self, _: &mut Ctx) {}

    fn is_busy(&self) -> bool {
        false
    }

    /// Прицел — место новой метки **(поведение)**: своего курсора-«пузыря», как в Figma, у ядра
    /// нет, а интерфейс знает только CSS-курсоры.
    fn cursor(&self) -> Cursor {
        Cursor::Crosshair
    }

    fn overlay(&self) -> ToolOverlay {
        ToolOverlay::default()
    }
}
