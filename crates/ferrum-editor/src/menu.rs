//! Главное и контекстные меню данными (спека подсистемы 2, разделы 3–5): дерево
//! команд по ключам таблицы, подменю и разделителей. Порядок и состав повторяют главное
//! меню Figma UI3 без плагинов, виджетов и Dev Mode. В дереве — только команды,
//! которые уже есть: пункты следующих этапов (вставить поверх, с заменой,
//! копирование свойств, линейки, выделить подобные) появятся вместе с ними, а не
//! серыми заглушками. Интерфейс получает дерево один раз и по пункту присылает
//! `Execute` с ключом команды.

use ferrum_doc::NodeId;

use crate::commands::{CommandGroup, CommandId};

/// Действие контекстного меню над его целью (слой, страница). Это не команда
/// таблицы: у него нет сочетания и смысла без цели, а выполняет его интерфейс
/// (поле имени, подтверждение удаления, ссылка в буфер обмена) — ядро только
/// говорит, доступно ли оно.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MenuAction {
    /// Переименовать слой (поле имени в его строке).
    RenameLayer,
    /// Развернуть все слои страницы.
    ExpandAll,
    /// Свернуть все слои страницы.
    CollapseAll,
    RenamePage,
    DuplicatePage,
    DeletePage,
    /// Ссылка на страницу (`#/file/<id>?page=<pageId>`) — в буфер обмена.
    CopyPageLink,
}

impl MenuAction {
    pub const ALL: [MenuAction; 7] = [
        MenuAction::RenameLayer,
        MenuAction::ExpandAll,
        MenuAction::CollapseAll,
        MenuAction::RenamePage,
        MenuAction::DuplicatePage,
        MenuAction::DeletePage,
        MenuAction::CopyPageLink,
    ];

    /// Название пункта.
    pub fn title(self) -> &'static str {
        match self {
            MenuAction::RenameLayer => "Переименовать",
            MenuAction::ExpandAll => "Развернуть всё",
            MenuAction::CollapseAll => "Свернуть всё",
            MenuAction::RenamePage => "Переименовать",
            MenuAction::DuplicatePage => "Дублировать страницу",
            MenuAction::DeletePage => "Удалить страницу",
            MenuAction::CopyPageLink => "Копировать ссылку на страницу",
        }
    }
}

/// Узел меню.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MenuNode {
    Command(CommandId),
    Submenu {
        title: &'static str,
        items: Vec<MenuNode>,
    },
    Separator,
    /// Пункт «Выделить слой»: узел под точкой щелчка и его имя.
    Node {
        id: NodeId,
        name: String,
    },
    /// Действие над целью контекстного меню и доступно ли оно.
    Action {
        action: MenuAction,
        enabled: bool,
    },
}

use CommandId as C;
use MenuNode::{Command as I, Separator as S};

pub(crate) fn sub(title: &'static str, items: Vec<MenuNode>) -> MenuNode {
    MenuNode::Submenu { title, items }
}

fn group(g: CommandGroup, items: Vec<MenuNode>) -> MenuNode {
    sub(g.title(), items)
}

/// Главное меню (кнопка с логотипом, Alt+/).
pub fn main_menu() -> Vec<MenuNode> {
    use CommandGroup as G;
    vec![
        group(
            G::File,
            vec![
                I(C::UiNewFile),
                I(C::UiOpen),
                I(C::UiOpenDisk),
                S,
                I(C::Save),
                I(C::UiSaveDisk),
                I(C::UiSaveCopy),
                S,
                I(C::UiExport),
                S,
                I(C::UiAllFiles),
            ],
        ),
        group(
            G::Edit,
            vec![
                I(C::Undo),
                I(C::Redo),
                S,
                I(C::Copy),
                I(C::Cut),
                I(C::Paste),
                sub("Копировать как", vec![I(C::CopyAsPng), I(C::CopyAsSvg)]),
                S,
                I(C::Duplicate),
                I(C::Delete),
                S,
                I(C::SelectAll),
            ],
        ),
        group(
            G::View,
            vec![
                I(C::PixelGrid),
                S,
                I(C::ToggleUi),
                S,
                I(C::ZoomIn),
                I(C::ZoomOut),
                I(C::Zoom100),
                I(C::ZoomToFit),
                I(C::ZoomToSelection),
            ],
        ),
        group(
            G::Object,
            vec![
                I(C::Group),
                I(C::Ungroup),
                I(C::FrameSelection),
                S,
                sub(
                    "Булевы операции",
                    vec![
                        I(C::BooleanUnion),
                        I(C::BooleanSubtract),
                        I(C::BooleanIntersect),
                        I(C::BooleanExclude),
                    ],
                ),
                I(C::Flatten),
                I(C::OutlineStroke),
                S,
                I(C::FlipHorizontal),
                I(C::FlipVertical),
                S,
                I(C::ToggleVisible),
                I(C::ToggleLock),
            ],
        ),
        group(
            G::Text,
            vec![
                I(C::TextBold),
                I(C::TextItalic),
                I(C::TextUnderline),
                I(C::TextStrike),
                S,
                sub(
                    "Выравнивание",
                    vec![
                        I(C::TextAlignLeft),
                        I(C::TextAlignCenter),
                        I(C::TextAlignRight),
                        I(C::TextAlignJustify),
                    ],
                ),
            ],
        ),
        group(
            G::Arrange,
            vec![
                I(C::BringToFront),
                I(C::BringForward),
                I(C::SendBackward),
                I(C::SendToBack),
            ],
        ),
        group(G::Vector, vec![I(C::EditObject), I(C::ToolPen)]),
        group(
            G::Settings,
            vec![
                sub(
                    "Тема",
                    vec![I(C::UiThemeLight), I(C::UiThemeDark), I(C::UiThemeSystem)],
                ),
                I(C::UiPixelSnap),
                S,
                I(C::UiShortcuts),
                I(C::UiSettings),
            ],
        ),
        group(G::Help, vec![I(C::UiShortcuts), I(C::UiAbout)]),
    ]
}

/// Все команды дерева по порядку (для проверок и поиска).
pub fn menu_commands(items: &[MenuNode]) -> Vec<CommandId> {
    let mut out = Vec::new();
    for n in items {
        match n {
            MenuNode::Command(c) => out.push(*c),
            MenuNode::Submenu { items, .. } => out.extend(menu_commands(items)),
            MenuNode::Separator | MenuNode::Node { .. } | MenuNode::Action { .. } => {}
        }
    }
    out
}
