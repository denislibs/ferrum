//! Состояние команд (спека подсистемы 2, раздел 3): доступна ли команда и
//! отмечена ли (у переключателей). Считает ядро — основной поток только рисует
//! меню, тулбар и палитру по таблице `ключ → состояние`. Интерфейс получает
//! только изменившиеся команды и только после шага, который мог их изменить
//! (выделение, история, инструмент, вид, режим правки, «только чтение»).
//! Недоступная команда при вызове ничего не делает: ни шага, ни сообщения.

use ferrum_doc::{BooleanOp, NodeKind};

use crate::commands::{CommandId, COMMANDS};
use crate::editor::{Editor, ThemeChoice};
use crate::selection::locked;

/// Состояние команды: доступна ли и (у переключателя) отмечена ли.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CommandState {
    pub enabled: bool,
    /// `None` — команда не переключатель.
    pub checked: Option<bool>,
}

/// Всё, от чего зависят состояния: пока ключ тот же, пересчитывать нечего.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct StatesKey {
    selection: u64,
    scene: u64,
    scene_epoch: u64,
    can_undo: bool,
    can_redo: bool,
    read_only: bool,
    text_edit: bool,
    vector_edit: bool,
    settings: crate::editor::Settings,
}

/// Что о выделении нужно состояниям — за один проход.
#[derive(Debug, Default)]
struct Facts {
    count: usize,
    /// Есть что разгруппировать: группа, фрейм или булева группа с детьми.
    ungroupable: bool,
    any_boolean: bool,
    /// Все выделенные — булевы группы с одной операцией.
    boolean_op: Option<BooleanOp>,
    has_text: bool,
    /// Один выделенный узел, у которого есть правка (текст, вектор, фигура).
    editable: bool,
}

/// Кэш отправленного интерфейсу: ключ и состояния в порядке `COMMANDS`.
#[derive(Debug, Default)]
pub(crate) struct StatesCache {
    key: Option<StatesKey>,
    sent: Vec<CommandState>,
}

impl Editor {
    fn states_key(&self) -> StatesKey {
        StatesKey {
            selection: self.selection.version(),
            scene: self.scene.change_counter(),
            scene_epoch: self.scene_epoch,
            can_undo: self.doc.can_undo(),
            can_redo: self.doc.can_redo(),
            read_only: self.doc.is_read_only(),
            text_edit: self.text_edit.is_some(),
            vector_edit: self.vector_edit.is_some(),
            settings: self.settings.clone(),
        }
    }

    fn facts(&self) -> Facts {
        let mut f = Facts {
            count: self.selection.len(),
            ..Default::default()
        };
        let mut same_op = true;
        for (i, id) in self.selection.iter().enumerate() {
            let Some(slot) = self.scene.slot(*id) else {
                continue;
            };
            let n = self.scene.at(slot);
            let kind = n.props.kind;
            if matches!(kind, NodeKind::Group | NodeKind::Frame | NodeKind::Boolean)
                && !n.children.is_empty()
            {
                f.ungroupable = true;
            }
            if kind == NodeKind::Boolean {
                f.any_boolean = true;
                let op = n.props.boolean_op.unwrap_or_default();
                if i == 0 {
                    f.boolean_op = Some(op);
                } else if f.boolean_op != Some(op) {
                    same_op = false;
                }
            } else {
                same_op = false;
            }
            if kind == NodeKind::Text {
                f.has_text = true;
            }
            if f.count == 1 {
                f.editable = matches!(
                    kind,
                    NodeKind::Text
                        | NodeKind::Vector
                        | NodeKind::Rect
                        | NodeKind::Ellipse
                        | NodeKind::Line
                ) && !locked(&self.scene, slot);
            }
            // Дальше ничего не изменится: проход по большому выделению кончается.
            if f.ungroupable && f.has_text && !same_op {
                break;
            }
        }
        if !same_op {
            f.boolean_op = None;
        }
        f
    }

    fn state_with(&self, id: CommandId, f: &Facts) -> CommandState {
        use CommandId as C;
        let ro = self.doc.is_read_only();
        let edit = !ro;
        let sel = f.count > 0;
        let text = self.text_edit.is_some();
        let vector = self.vector_edit.is_some();
        let checked = |on: bool| Some(on);
        let (enabled, checked) = match id {
            C::ToolMove
            | C::ToolFrame
            | C::ToolRect
            | C::ToolEllipse
            | C::ToolLine
            | C::ToolHand
            | C::ToolText
            | C::ToolPen
            | C::ToolArrow
            | C::ToolScale
            | C::HandHold
            | C::ToolImage
            | C::SelectAll
            | C::Escape
            | C::ZoomToFit
            | C::Zoom100
            | C::ZoomIn
            | C::ZoomOut
            | C::Save => (true, None),
            // Путь пера отменяется по точке, даже если истории ещё нет.
            C::Undo => (self.doc.can_undo() || vector, None),
            C::Redo => (self.doc.can_redo(), None),
            C::Copy => (sel || text, None),
            C::Cut => (edit && (sel || text), None),
            C::Paste => (edit, None),
            C::CopyAsPng | C::CopyAsSvg | C::ZoomToSelection => (sel, None),
            // В правке вектора удаление и сдвиг — у точек.
            C::Delete
            | C::NudgeLeft
            | C::NudgeRight
            | C::NudgeUp
            | C::NudgeDown
            | C::NudgeLeftBig
            | C::NudgeRightBig
            | C::NudgeUpBig
            | C::NudgeDownBig => (edit && (sel || vector), None),
            C::Duplicate
            | C::Group
            | C::FrameSelection
            | C::Flatten
            | C::OutlineStroke
            | C::BringForward
            | C::SendBackward
            | C::BringToFront
            | C::SendToBack
            | C::ToggleVisible
            | C::ToggleLock
            | C::FlipHorizontal
            | C::FlipVertical => (edit && sel, None),
            C::Ungroup => (edit && f.ungroupable, None),
            // Как в Figma: нужны хотя бы два слоя или выделенные булевы группы (смена
            // операции); отмечена операция выделенных групп.
            C::BooleanUnion | C::BooleanSubtract | C::BooleanIntersect | C::BooleanExclude => {
                let op = match id {
                    C::BooleanUnion => BooleanOp::Union,
                    C::BooleanSubtract => BooleanOp::Subtract,
                    C::BooleanIntersect => BooleanOp::Intersect,
                    _ => BooleanOp::Exclude,
                };
                (
                    edit && (f.count >= 2 || f.any_boolean),
                    checked(f.boolean_op == Some(op)),
                )
            }
            C::ToggleUi => (true, checked(self.settings.ui_hidden)),
            C::PixelGrid => (true, checked(self.settings.pixel_grid)),
            C::EditObject => (edit && !text && !vector && f.editable, None),
            C::TextBold
            | C::TextItalic
            | C::TextUnderline
            | C::TextStrike
            | C::TextAlignLeft
            | C::TextAlignCenter
            | C::TextAlignRight
            | C::TextAlignJustify => (edit && (text || f.has_text), None),
            C::UiPixelSnap => (true, checked(self.settings.pixel_snap)),
            C::UiThemeDark => (true, checked(self.settings.theme == ThemeChoice::Dark)),
            C::UiThemeLight => (true, checked(self.settings.theme == ThemeChoice::Light)),
            C::UiThemeSystem => (true, checked(self.settings.theme == ThemeChoice::System)),
            C::UiMainMenu
            | C::UiNewFile
            | C::UiOpen
            | C::UiOpenDisk
            | C::UiSaveDisk
            | C::UiSaveCopy
            | C::UiExport
            | C::UiAllFiles
            | C::UiSettings
            | C::UiShortcuts
            | C::UiAbout => (true, None),
        };
        CommandState { enabled, checked }
    }

    /// Состояние одной команды сейчас.
    pub fn command_state(&self, id: CommandId) -> CommandState {
        self.state_with(id, &self.facts())
    }

    /// Состояния всех команд в порядке `COMMANDS` (один проход по выделению).
    pub fn command_states(&self) -> Vec<CommandState> {
        let f = self.facts();
        COMMANDS.iter().map(|c| self.state_with(c.id, &f)).collect()
    }

    /// Изменившиеся с прошлого раза состояния (в первый раз и после
    /// `reset_command_states` — все); `None` — ничего не изменилось или ключ тот
    /// же. Во время жеста не считаются: состояния не меняются посреди него.
    pub fn take_command_states(&mut self) -> Option<Vec<(CommandId, CommandState)>> {
        if self.is_busy() {
            return None;
        }
        let key = self.states_key();
        if self.states.key.as_ref() == Some(&key) {
            return None;
        }
        let now = self.command_states();
        let changed: Vec<(CommandId, CommandState)> = COMMANDS
            .iter()
            .zip(&now)
            .enumerate()
            .filter(|(i, (_, s))| self.states.sent.get(*i) != Some(s))
            .map(|(_, (c, s))| (c.id, *s))
            .collect();
        self.states = StatesCache {
            key: Some(key),
            sent: now,
        };
        (!changed.is_empty()).then_some(changed)
    }

    /// Интерфейс забыл состояния (новый документ, новое ядро): следующий
    /// `take_command_states` вернёт все.
    pub fn reset_command_states(&mut self) {
        self.states = StatesCache::default();
    }
}
