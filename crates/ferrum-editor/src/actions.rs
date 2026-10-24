//! Команды из таблицы. На этом шаге — инструменты, отмена, повтор и Esc;
//! правки выделения, порядок слоёв, вид и буфер обмена добавляет следующая задача.

use crate::commands::CommandId;
use crate::editor::{Editor, EditorEvent};
use crate::tools::ToolKind;

impl Editor {
    pub(crate) fn run(&mut self, cmd: CommandId) {
        use CommandId as C;
        match cmd {
            C::ToolMove => self.set_tool(ToolKind::Move),
            C::ToolFrame => self.set_tool(ToolKind::Frame),
            C::ToolRect => self.set_tool(ToolKind::Rect),
            C::ToolEllipse => self.set_tool(ToolKind::Ellipse),
            C::ToolLine => self.set_tool(ToolKind::Line),
            C::ToolHand => self.set_tool(ToolKind::Hand),
            // Удержание обрабатывается при нажатии и отпускании клавиши.
            C::HandHold => {}
            C::ToolImage => self.out.events.push(EditorEvent::RequestImage),
            C::Undo => self.undo_redo(false),
            C::Redo => self.undo_redo(true),
            C::Escape => {
                if self.tool.kind() != ToolKind::Move {
                    self.set_tool(ToolKind::Move);
                } else {
                    self.selection.clear();
                }
            }
            _ => {}
        }
    }
}
