//! Правки из панелей (этап 5): поля и палитра панели свойств, кнопки и
//! переименование в слоях, перетаскивание слоёв. Каждая правка — одна транзакция,
//! то есть один шаг отмены.
//!
//! Перетаскивание подписи поля и движение в палитре цвета — жест, как на холсте:
//! пока он идёт, изменения живут только в превью сцены (`Phase::Preview`), а
//! отпускание записывает одну транзакцию (`Phase::Commit`); `Phase::Cancel`
//! (Esc) убирает превью без следа в истории. Каждое превью считается от свойств
//! на начало жеста, поэтому относительные правки (`relative`) не накапливаются.

use std::collections::HashSet;

use ferrum_doc::{Fill, NodeId, NodeKind, Props, PropsPatch, Stroke};
use ferrum_scene::shape::COORD_LIMIT;
use ferrum_scene::{Scene, Slot};
use kurbo::{Affine, Point, Rect};

use crate::editor::Editor;
use crate::geometry::{
    bases, normalize_deg, parent_world, rigid_patches, scale_patches_in, top_level, Base,
    PixelSnap, MIN_SIDE,
};
use crate::ops::reposition;
use crate::pages::clean_name;

/// Числовое поле панели свойств.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NumField {
    X,
    Y,
    W,
    H,
    /// Градусы против часовой стрелки, как в Figma.
    Rotation,
    /// Прозрачность слоя, от 0 до 1.
    Opacity,
    CornerRadius,
    /// Толщина всех обводок узла.
    StrokeWidth,
}

/// Правка выделения из панели свойств.
#[derive(Debug, Clone, PartialEq)]
pub enum PanelEdit {
    /// Значение поля у каждого выделенного узла; `relative` — прибавить к его
    /// собственному значению (перетаскивание подписи у «Смешанных»).
    Number {
        field: NumField,
        value: f64,
        relative: bool,
    },
    /// Заменить заливки целиком (у групп и линий заливок нет — пропускаются).
    Fills(Vec<Fill>),
    /// Заменить обводки целиком (у групп обводок нет).
    Strokes(Vec<Stroke>),
    /// Обрезка содержимого — только у фреймов.
    ClipContent(bool),
    /// Ширина и высота сразу (пресет фрейма) — одним шагом.
    Size { w: f64, h: f64 },
}

/// Фаза правки: превью жеста, запись одним шагом или отмена жеста.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    Preview,
    Commit,
    Cancel,
}

/// Значения полей «Положение» узла, как их показывает панель: у группы — рамка её
/// детей в координатах родителя, у остальных — свойства узла. Поворот — в
/// `(-180, 180]`, как в Figma.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PanelPose {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
    pub rotation: f64,
}

/// Поворот для панели: `[0, 360)` → `(-180, 180]`.
pub fn display_rotation(deg: f64) -> f64 {
    let r = normalize_deg(deg);
    if r > 180.0 {
        r - 360.0
    } else {
        r
    }
}

pub fn panel_pose(scene: &Scene, slot: Slot) -> PanelPose {
    let n = scene.at(slot);
    let p = &n.props;
    if p.kind == NodeKind::Group {
        let b = n.local_bounds.unwrap_or(Rect::ZERO);
        let corner = n.local * Point::new(b.x0, b.y0);
        return PanelPose {
            x: corner.x,
            y: corner.y,
            w: b.width(),
            h: b.height(),
            rotation: display_rotation(p.rotation),
        };
    }
    PanelPose {
        x: p.x,
        y: p.y,
        w: p.w,
        h: p.h,
        rotation: display_rotation(p.rotation),
    }
}

/// Может ли поле относиться к узлу этого типа.
pub fn field_applies(field: NumField, kind: NodeKind) -> bool {
    match field {
        NumField::H => kind != NodeKind::Line,
        NumField::CornerRadius => {
            matches!(kind, NodeKind::Frame | NodeKind::Rect | NodeKind::Image)
        }
        NumField::StrokeWidth => kind != NodeKind::Group,
        _ => true,
    }
}

/// Узел на начало жеста панели: свойства документа, матрицы и рамка.
#[derive(Debug, Clone)]
struct Target {
    id: NodeId,
    props: Props,
    local: Affine,
    parent_world: Affine,
    world: Affine,
    bounds: Option<Rect>,
    pose: PanelPose,
    /// Узел верхнего уровня выделения: ни один его предок не выделен. Геометрию
    /// (сдвиг, поворот, размер) меняют только такие узлы — потомки выделенного
    /// предка двигаются вместе с ним, второй раз их трогать нельзя.
    top: bool,
    /// База узла; с `deep` у группы — и поддерево для масштаба. Пусто у узлов не
    /// верхнего уровня.
    bases: Vec<Base>,
}

/// Жест панели: узлы и их состояние на начало.
#[derive(Debug, Clone, Default)]
pub(crate) struct PanelGesture {
    targets: Vec<Target>,
    /// Базы групп собраны вместе с поддеревом (нужно только для размера).
    deep: bool,
}

/// Правке нужно поддерево групп: ширина и высота масштабируют детей группы.
fn needs_children(edit: &PanelEdit) -> bool {
    matches!(
        edit,
        PanelEdit::Size { .. }
            | PanelEdit::Number {
                field: NumField::W | NumField::H,
                ..
            }
    )
}

/// Правка меняет положение, поворот или размер — её не применяют к потомкам
/// выделенного предка, иначе они сдвинутся дважды.
fn is_geometric(edit: &PanelEdit) -> bool {
    match edit {
        PanelEdit::Number {
            field, relative, ..
        } => match field {
            NumField::X | NumField::Y => *relative,
            NumField::W | NumField::H | NumField::Rotation => true,
            _ => false,
        },
        PanelEdit::Size { .. } => true,
        _ => false,
    }
}

fn capture(scene: &Scene, ids: &[NodeId], deep: bool) -> PanelGesture {
    let live: Vec<NodeId> = ids
        .iter()
        .copied()
        .filter(|id| !scene.is_draft(*id) && scene.slot(*id).is_some())
        .collect();
    let top: HashSet<NodeId> = top_level(scene, &live).into_iter().collect();
    let targets = live
        .iter()
        .filter_map(|id| scene.slot(*id).map(|s| (*id, s)))
        .map(|(id, slot)| {
            let n = scene.at(slot);
            let is_top = top.contains(&id);
            Target {
                id,
                props: n.props.clone(),
                local: n.local,
                parent_world: parent_world(scene, slot),
                world: n.world,
                bounds: n.local_bounds,
                pose: panel_pose(scene, slot),
                top: is_top,
                bases: if is_top {
                    bases(scene, &[id], deep)
                } else {
                    vec![]
                },
            }
        })
        .collect();
    PanelGesture { targets, deep }
}

fn finite_or(v: f64, base: f64) -> f64 {
    if v.is_finite() {
        v
    } else {
        base
    }
}

/// Новое значение поля: абсолютное или прибавка к своему.
fn target_value(base: f64, value: f64, relative: bool) -> f64 {
    finite_or(if relative { base + value } else { value }, base)
}

/// Новые ширина и (или) высота узла. У фрейма, фигуры и изображения меняются
/// `w`, `h`, левый верхний угол в осях узла остаётся на месте (как в Figma). У
/// группы своих размеров нет: масштабируются дети в её осях от левого верхнего
/// угла рамки.
fn size_patches(t: &Target, w: Option<f64>, h: Option<f64>) -> Vec<(NodeId, PropsPatch)> {
    let side = |v: f64| v.clamp(MIN_SIDE, COORD_LIMIT);
    let w = w.filter(|v| v.is_finite()).map(side);
    let h = h
        .filter(|v| v.is_finite() && t.props.kind != NodeKind::Line)
        .map(side);
    if w.is_none() && h.is_none() {
        return vec![];
    }
    if t.props.kind != NodeKind::Group {
        return vec![(
            t.id,
            PropsPatch {
                w,
                h,
                ..Default::default()
            },
        )];
    }
    let Some(b) = t.bounds.filter(|b| b.width() > 0.0 && b.height() > 0.0) else {
        return vec![];
    };
    let sx = w.map_or(1.0, |w| w / b.width());
    let sy = h.map_or(1.0, |h| h / b.height());
    let local = Affine::translate((b.x0, b.y0))
        * Affine::scale_non_uniform(sx, sy)
        * Affine::translate((-b.x0, -b.y0));
    let world = t.world * local * t.world.inverse();
    scale_patches_in(&t.bases, world, t.world, PixelSnap { x: false, y: false })
}

/// Правки свойств одного узла для числового поля.
fn number_patches(
    t: &Target,
    field: NumField,
    value: f64,
    relative: bool,
) -> Vec<(NodeId, PropsPatch)> {
    if !value.is_finite() || !field_applies(field, t.props.kind) {
        return vec![];
    }
    let p = &t.props;
    let one = |patch: PropsPatch| vec![(t.id, patch)];
    match field {
        NumField::X | NumField::Y => {
            let (base, cur) = if field == NumField::X {
                (t.pose.x, p.x)
            } else {
                (t.pose.y, p.y)
            };
            let next =
                (cur + target_value(base, value, relative) - base).clamp(-COORD_LIMIT, COORD_LIMIT);
            one(if field == NumField::X {
                PropsPatch {
                    x: Some(next),
                    ..Default::default()
                }
            } else {
                PropsPatch {
                    y: Some(next),
                    ..Default::default()
                }
            })
        }
        NumField::W => size_patches(t, Some(target_value(t.pose.w, value, relative)), None),
        NumField::H => size_patches(t, None, Some(target_value(t.pose.h, value, relative))),
        NumField::Rotation => {
            // Дельта — кратчайший поворот: полный оборот не правка.
            let delta =
                display_rotation(target_value(t.pose.rotation, value, relative) - t.pose.rotation);
            if delta.abs() < 1e-9 {
                return vec![];
            }
            // Поворот вокруг центра рамки (как в Figma); в модели — против часовой
            // стрелки вокруг левого верхнего угла, поэтому меняются и x, y.
            let c = t.local * t.bounds.unwrap_or(Rect::ZERO).center();
            let in_parent = Affine::translate(c.to_vec2())
                * Affine::rotate(-delta.to_radians())
                * Affine::translate(-c.to_vec2());
            let world = t.parent_world * in_parent * t.parent_world.inverse();
            let top: Vec<Base> = t.bases.iter().take(1).cloned().collect();
            rigid_patches(&top, world, false)
        }
        NumField::Opacity => one(PropsPatch {
            opacity: Some(target_value(p.opacity, value, relative).clamp(0.0, 1.0)),
            ..Default::default()
        }),
        NumField::CornerRadius => one(PropsPatch {
            corner_radius: Some(
                target_value(p.corner_radius, value, relative).clamp(0.0, COORD_LIMIT),
            ),
            ..Default::default()
        }),
        NumField::StrokeWidth => {
            if p.strokes.is_empty() {
                return vec![];
            }
            let strokes = p
                .strokes
                .iter()
                .map(|s| Stroke {
                    width: target_value(s.width, value, relative).clamp(0.0, COORD_LIMIT),
                    ..s.clone()
                })
                .collect();
            one(PropsPatch {
                strokes: Some(strokes),
                ..Default::default()
            })
        }
    }
}

fn patches(g: &PanelGesture, edit: &PanelEdit) -> Vec<(NodeId, PropsPatch)> {
    let mut out = Vec::new();
    let geometric = is_geometric(edit);
    for t in &g.targets {
        if geometric && !t.top {
            continue;
        }
        let kind = t.props.kind;
        match edit {
            PanelEdit::Number {
                field,
                value,
                relative,
            } => out.extend(number_patches(t, *field, *value, *relative)),
            PanelEdit::Fills(fills) => {
                if kind != NodeKind::Group && kind != NodeKind::Line {
                    out.push((
                        t.id,
                        PropsPatch {
                            fills: Some(fills.clone()),
                            ..Default::default()
                        },
                    ));
                }
            }
            PanelEdit::Strokes(strokes) => {
                if kind != NodeKind::Group {
                    out.push((
                        t.id,
                        PropsPatch {
                            strokes: Some(strokes.clone()),
                            ..Default::default()
                        },
                    ));
                }
            }
            PanelEdit::Size { w, h } => {
                if w.is_finite() && h.is_finite() {
                    let h = field_applies(NumField::H, kind).then_some(*h);
                    out.extend(size_patches(t, Some(*w), h));
                }
            }
            PanelEdit::ClipContent(on) => {
                if kind == NodeKind::Frame {
                    out.push((
                        t.id,
                        PropsPatch {
                            clip_content: Some(*on),
                            ..Default::default()
                        },
                    ));
                }
            }
        }
    }
    debug_assert!(
        {
            let mut seen = HashSet::new();
            out.iter().all(|(id, _)| seen.insert(*id))
        },
        "узел правится дважды"
    );
    out
}

/// Куда положить слои, перетащенные в панели слоёв (строки сверху вниз — от
/// переднего плана к заднему).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DropTarget {
    /// Над строкой: перед узлом, у того же родителя.
    Above(NodeId),
    /// Под строкой: за узлом, у того же родителя.
    Below(NodeId),
    /// Внутрь фрейма или группы, поверх его детей.
    Into(NodeId),
}

impl Editor {
    /// Правка выделения из панели свойств (см. описание модуля). Жест холста,
    /// если он идёт, отменяется.
    pub fn edit_selection(&mut self, edit: &PanelEdit, phase: Phase) {
        self.edit_targets(edit, phase, None);
    }

    /// То же для заданных узлов, а не текущего выделения: поле правило их, пока
    /// выделение успело смениться (щелчок по холсту, строка слоёв). Мёртвые узлы
    /// отбрасываются.
    pub fn edit_nodes(&mut self, edit: &PanelEdit, phase: Phase, ids: &[NodeId]) {
        self.edit_targets(edit, phase, Some(ids));
    }

    fn edit_targets(&mut self, edit: &PanelEdit, phase: Phase, ids: Option<&[NodeId]>) {
        let targets = |me: &Self| match ids {
            Some(ids) => me.alive(ids),
            None => me.selection.to_vec(),
        };
        match phase {
            Phase::Cancel => self.cancel_panel(),
            Phase::Preview => {
                let deep = needs_children(edit);
                // Жест начался с правки без поддерева групп, а теперь оно нужно:
                // превью снимается, базы собираются заново.
                if self.panel.as_ref().is_some_and(|g| deep && !g.deep) {
                    self.cancel_panel();
                }
                if self.panel.is_none() {
                    self.cancel_gesture();
                    self.panel = Some(capture(&self.scene, &targets(self), deep));
                }
                let p = patches(self.panel.as_ref().expect("задан выше"), edit);
                self.scene.set_preview(&p);
            }
            Phase::Commit => {
                let deep = needs_children(edit);
                let g = match self.panel.take() {
                    Some(g) if g.deep || !deep => g,
                    _ => {
                        // Адресат задан ids — жест холста, начавшийся позже, не трогаем.
                        if ids.is_none() {
                            self.cancel_gesture();
                        }
                        self.scene.clear_preview();
                        capture(&self.scene, &targets(self), deep)
                    }
                };
                let p = patches(&g, edit);
                self.scene.clear_preview();
                if !p.is_empty() {
                    let selection = self.selection.to_vec();
                    self.ctx().commit(|tx, _| {
                        for (id, patch) in &p {
                            crate::text::set_props_resizing(tx, *id, patch)?;
                        }
                        Ok(selection)
                    });
                }
            }
        }
        self.emit();
    }

    /// Убирает превью жеста панели без записи.
    pub(crate) fn cancel_panel(&mut self) {
        if self.panel.take().is_some() {
            self.scene.clear_preview();
        }
    }

    /// Скрыть или заблокировать узлы (кнопки строки слоя) одним шагом.
    pub fn set_flags(&mut self, ids: &[NodeId], visible: Option<bool>, locked: Option<bool>) {
        self.cancel_gesture();
        let ids = self.alive(ids);
        if ids.is_empty() || (visible.is_none() && locked.is_none()) {
            return;
        }
        let patch = PropsPatch {
            visible,
            locked,
            ..Default::default()
        };
        let selection = self.selection.to_vec();
        self.ctx().commit(|tx, _| {
            for id in &ids {
                tx.set_props(*id, &patch)?;
            }
            Ok(selection)
        });
        self.emit();
    }

    /// Переименование слоя одним шагом; пустое или то же имя ничего не меняет.
    pub fn rename_node(&mut self, id: NodeId, name: &str) {
        self.cancel_gesture();
        let Some(name) = clean_name(name) else {
            return;
        };
        if self.scene.node(id).is_none_or(|n| n.props.name == name) || self.scene.is_draft(id) {
            return;
        }
        let selection = self.selection.to_vec();
        self.ctx().commit(|tx, _| {
            tx.set_props(
                id,
                &PropsPatch {
                    name: Some(name),
                    ..Default::default()
                },
            )?;
            Ok(selection)
        });
        self.emit();
    }

    /// Порядок и вложенность из панели слоёв: узлы `ids` (без тех, у кого выделен
    /// предок) встают блоком к цели в прежнем взаимном порядке и остаются на месте
    /// на экране. Один шаг; перенос узла внутрь самого себя или в лист ничего не
    /// делает. Возвращает, изменился ли документ.
    pub fn move_layers(&mut self, ids: &[NodeId], target: DropTarget) -> bool {
        self.cancel_gesture();
        let ids = top_level(&self.scene, &self.alive(ids));
        let anchor = match target {
            DropTarget::Above(a) | DropTarget::Below(a) | DropTarget::Into(a) => a,
        };
        let Some(anchor_slot) = self.scene.slot(anchor) else {
            return false;
        };
        let parent = match target {
            DropTarget::Into(a) => {
                let kind = self.scene.at(anchor_slot).props.kind;
                if kind != NodeKind::Frame && kind != NodeKind::Group {
                    return false;
                }
                Some(a)
            }
            _ => self
                .scene
                .at(anchor_slot)
                .parent
                .map(|p| self.scene.at(p).id),
        };
        // Цель не может лежать внутри переносимых (и быть одним из них при «внутрь»).
        let moving: HashSet<NodeId> = ids.iter().copied().collect();
        let mut cur = parent.and_then(|p| self.scene.slot(p));
        while let Some(s) = cur {
            if moving.contains(&self.scene.at(s).id) {
                return false;
            }
            cur = self.scene.at(s).parent;
        }
        if ids.is_empty() || (target != DropTarget::Into(anchor) && moving.contains(&anchor)) {
            return false;
        }
        // Блок — в порядке отрисовки (снизу вверх), как стоят сейчас.
        let mut block: Vec<(Vec<u32>, NodeId)> = ids
            .iter()
            .map(|id| {
                (
                    self.scene.order_key(self.scene.slot(*id).expect("живой")),
                    *id,
                )
            })
            .collect();
        block.sort_by(|a, b| a.0.cmp(&b.0));
        let block: Vec<NodeId> = block.into_iter().map(|(_, id)| id).collect();
        let parent_world = parent
            .and_then(|p| self.scene.slot(p))
            .map_or(Affine::IDENTITY, |s| self.scene.at(s).world);
        // Положение на экране: узлам, у которых меняется родитель, — новые x, y,
        // поворот и отражение в координатах нового родителя.
        let repos: Vec<(NodeId, PropsPatch)> = block
            .iter()
            .filter_map(|id| {
                let s = self.scene.slot(*id)?;
                let n = self.scene.at(s);
                let old = n.parent.map(|p| self.scene.at(p).id);
                (old != parent).then(|| (*id, reposition(parent_world, n.world, &n.props)))
            })
            .collect();
        // Позиция блока среди остальных детей цели.
        let place = |others: &[NodeId]| match target {
            DropTarget::Into(_) => others.len(),
            DropTarget::Above(a) => others
                .iter()
                .position(|c| *c == a)
                .map_or(others.len(), |i| i + 1),
            DropTarget::Below(a) => others.iter().position(|c| *c == a).unwrap_or(0),
        };
        // Бросили туда, где слои и стоят, — правки нет (и шага отмены тоже).
        let current = self.page_doc().children(parent);
        let mut want: Vec<NodeId> = current
            .iter()
            .copied()
            .filter(|c| !moving.contains(c))
            .collect();
        let at = place(&want);
        want.splice(at..at, block.iter().copied());
        if repos.is_empty() && want == current {
            return false;
        }
        let steps = self.out.steps;
        let selection = block.clone();
        self.ctx().commit(|tx, _| {
            let others: Vec<NodeId> = tx
                .children(parent)
                .into_iter()
                .filter(|c| !moving.contains(c))
                .collect();
            tx.move_nodes(&block, parent, place(&others))?;
            for (id, patch) in &repos {
                tx.set_props(*id, patch)?;
            }
            Ok(selection)
        });
        self.emit();
        self.out.steps != steps
    }
}
