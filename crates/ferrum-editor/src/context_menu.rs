//! Контекстные меню (спека подсистемы 2, раздел 5): что показать по правому
//! щелчку на холсте, по строке слоя и по странице. Сначала — выбор, как в Figma
//! на правой кнопке: невыделенный узел под точкой (или строка слоя) выделяется,
//! выделенный сохраняет выделение. Затем — дерево пунктов: команды таблицы (их
//! доступность интерфейс берёт из `CommandStates`), «Выделить слой» с узлами под
//! точкой и действия над целью (переименовать, развернуть всё, страницы).
//!
//! В дереве — только команды, которые уже есть: пункты следующих этапов
//! (линейки, недавние шрифты) появятся вместе с ними.

use ferrum_doc::{NodeId, NodeKind, PageId};
use kurbo::Point;

use crate::commands::CommandId;
use crate::editor::Editor;
use crate::geometry::SelectionBox;
use crate::menu::{sub, MenuAction, MenuNode};
use crate::selection::{frame_label_at, resolve, top_hit, Pick};
use crate::tools::{ToolKind, HIT_TOLERANCE_PX};

/// Сколько узлов под точкой показывает «Выделить слой».
pub const MAX_SELECT_LAYER_ITEMS: usize = 50;

/// По чему щёлкнули правой кнопкой.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ContextTarget {
    /// Точка холста в CSS-пикселях (как у событий указателя).
    Canvas(Point),
    /// Строка панели слоёв.
    Layer(NodeId),
    /// Строка списка страниц.
    Page(PageId),
}

use CommandId as C;
use MenuNode::{Command as I, Separator as S};

fn action(action: MenuAction, enabled: bool) -> MenuNode {
    MenuNode::Action { action, enabled }
}

/// Название узла для пункта «Выделить слой»: имя, а без него — тип, как в
/// панели слоёв.
fn node_name(name: &str, kind: NodeKind) -> String {
    if !name.trim().is_empty() {
        return name.to_string();
    }
    match kind {
        NodeKind::Frame => "Фрейм",
        NodeKind::Group => "Группа",
        NodeKind::Rect => "Прямоугольник",
        NodeKind::Ellipse => "Эллипс",
        NodeKind::Line => "Линия",
        NodeKind::Image => "Изображение",
        NodeKind::Text => "Текст",
        NodeKind::Vector => "Вектор",
        NodeKind::Boolean => "Булева группа",
    }
    .to_string()
}

impl Editor {
    /// Контекстное меню цели: выбор на правой кнопке и дерево пунктов. Пустое
    /// дерево — меню не открывать (цель не найдена, идёт жест).
    pub fn context_menu(&mut self, target: ContextTarget) -> Vec<MenuNode> {
        // Жест панели держит узлы на начало жеста: выбор мог бы их сменить.
        self.cancel_panel();
        if self.is_busy() {
            return Vec::new();
        }
        let items = match target {
            ContextTarget::Canvas(p) => {
                // В режиме «Комментарий» правый щелчок по холсту меню не открывает и
                // выделения не меняет **(поведение, как в Figma)**: выделения и правок
                // узлов в режиме не бывает, а пунктов без узла (вставка, «Выделить
                // всё») в нём тоже нет — меню вышло бы пустым или с одним видом.
                if self.tool.kind() == ToolKind::Comment {
                    return Vec::new();
                }
                // Над линейкой выбора нет: меню — по нынешнему выделению.
                let under = if self.over_ruler(p).is_some() {
                    None
                } else {
                    self.context_select_at(p)
                };
                self.node_menu(under, false)
            }
            ContextTarget::Layer(id) => {
                if self.scene.slot(id).is_none() || self.scene.is_draft(id) {
                    return Vec::new();
                }
                // Правый щелчок по невыделенной строке выделяет её (как в Figma).
                if !self.selection.contains(&id) {
                    self.select(&[id]);
                }
                self.node_menu(None, true)
            }
            ContextTarget::Page(id) => {
                if !self.doc.pages().iter().any(|(p, _)| *p == id) {
                    return Vec::new();
                }
                self.page_menu()
            }
        };
        self.emit();
        items
    }

    /// Выбор правой кнопкой в экранной точке `p`, как в Figma: узел под точкой,
    /// который выбрал бы щелчок, выделяется, если он не выделен; выделенный
    /// (или пустое место внутри рамки выделения) выделение сохраняет; пустое
    /// место вне её снимает выделение. Правку текста и вектора правый щелчок не
    /// заканчивает: меню — по их узлу. Возвращает узлы под точкой для «Выделить
    /// слой», сверху вниз.
    fn context_select_at(&mut self, p: Point) -> Option<Vec<(NodeId, String)>> {
        let world = self.camera.screen_to_world(p);
        let tolerance = HIT_TOLERANCE_PX / self.camera.zoom();
        let under = self.nodes_under(world, tolerance);
        if self.text_edit.is_some() || self.vector_edit.is_some() {
            return Some(under);
        }
        let picked = frame_label_at(&self.scene, &self.camera, p).or_else(|| {
            let hit = top_hit(&self.scene, world, tolerance)?;
            resolve(&self.scene, hit, &self.selection, Pick::Normal)
        });
        match picked {
            Some(id) if self.selection.contains(&id) => {}
            Some(id) => self.selection.set(vec![id]),
            None => {
                let inside = SelectionBox::of(&self.scene, &self.selection)
                    .is_some_and(|b| b.rect.contains(b.to_world.inverse() * world));
                if !inside {
                    self.selection.clear();
                }
            }
        }
        Some(under)
    }

    /// Все узлы под точкой мира, от верхнего к нижнему, с предками (как «Select
    /// layer» в Figma): у каждого попадания — оно само, затем его контейнеры.
    /// Черновики пропускаются; не больше `MAX_SELECT_LAYER_ITEMS`.
    fn nodes_under(&self, world: Point, tolerance: f64) -> Vec<(NodeId, String)> {
        let mut out: Vec<(NodeId, String)> = Vec::new();
        'hits: for id in self.scene.hit_test_all(world, tolerance) {
            let mut cur = self.scene.slot(id);
            while let Some(s) = cur {
                let n = self.scene.at(s);
                if !self.scene.is_draft(n.id) && !out.iter().any(|(x, _)| *x == n.id) {
                    if out.len() == MAX_SELECT_LAYER_ITEMS {
                        break 'hits;
                    }
                    out.push((n.id, node_name(&n.props.name, n.props.kind)));
                }
                cur = n.parent;
            }
        }
        out
    }

    /// Меню выделения (холст и строка слоя) или пустого места холста. Блоки — в
    /// порядке Figma (спека, раздел 5); `under` — узлы под точкой щелчка для
    /// «Выделить слой», `layer` — меню строки слоя.
    fn node_menu(&self, under: Option<Vec<(NodeId, String)>>, layer: bool) -> Vec<MenuNode> {
        if self.selection.is_empty() {
            // Пустое место холста.
            return vec![
                I(C::Paste),
                I(C::PasteOver),
                S,
                I(C::SelectAll),
                I(C::ZoomToFit),
                S,
                I(C::ViewRulers),
            ];
        }
        let mut items = vec![
            I(C::Copy),
            I(C::Paste),
            I(C::PasteOver),
            I(C::PasteReplace),
            sub("Копировать как", vec![I(C::CopyAsPng), I(C::CopyAsSvg)]),
            S,
            I(C::CopyProps),
            I(C::PasteProps),
        ];
        if let Some(under) = under.filter(|u| !u.is_empty()) {
            items.push(S);
            items.push(sub(
                "Выделить слой",
                under
                    .into_iter()
                    .map(|(id, name)| MenuNode::Node { id, name })
                    .collect(),
            ));
        }
        items.extend([
            S,
            I(C::BringForward),
            I(C::SendBackward),
            I(C::BringToFront),
            I(C::SendToBack),
            S,
            I(C::Group),
            I(C::Ungroup),
            I(C::FrameSelection),
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
            I(C::ToggleVisible),
            I(C::ToggleLock),
        ]);
        // Только у вектора и фигуры: «Войти в правку».
        let shape = match self.selection[..] {
            [one] => self.scene.node(one).is_some_and(|n| {
                matches!(
                    n.props.kind,
                    NodeKind::Vector | NodeKind::Rect | NodeKind::Ellipse | NodeKind::Line
                )
            }),
            _ => false,
        };
        if shape {
            items.extend([S, I(C::EditObject)]);
        }
        if layer {
            let edit = !self.doc.is_read_only();
            items.extend([
                S,
                action(MenuAction::RenameLayer, edit),
                action(MenuAction::ExpandAll, true),
                action(MenuAction::CollapseAll, true),
            ]);
        }
        items.extend([S, I(C::Delete)]);
        items
    }

    /// Меню страницы: переименовать, дублировать, удалить (не единственную),
    /// ссылка на страницу. Правки страниц в документе только для чтения
    /// недоступны.
    fn page_menu(&self) -> Vec<MenuNode> {
        let edit = !self.doc.is_read_only();
        let many = self.doc.pages().len() > 1;
        vec![
            action(MenuAction::RenamePage, edit),
            action(MenuAction::DuplicatePage, edit),
            action(MenuAction::DeletePage, edit && many),
            S,
            action(MenuAction::CopyPageLink, true),
        ]
    }
}
