//! Команды из таблицы: правки выделения, порядок слоёв, вид, буфер обмена.
//! Каждая правка документа — одна транзакция, то есть один шаг отмены.

use std::collections::BTreeSet;

use ferrum_doc::{Fill, NodeId, NodeKind, Props, PropsPatch};
use kurbo::{Affine, Point, Rect, Vec2};

use crate::clipboard::{full_patch, ClipNode, ClipPayload, ClipboardError};
use crate::commands::CommandId;
use crate::editor::{Editor, EditorEvent};
use crate::geometry::{decompose, parent_world, top_level, world_box};
use crate::ops::{duplicate, local_delta, reposition};
use crate::presets::FramePreset;
use crate::selection::locked;
use crate::tools::ToolKind;

/// Отступ «Показать всё» и «Показать выделение» в CSS-пикселях.
pub const FIT_PADDING_PX: f64 = 48.0;
/// Зазор между новым фреймом из пресета и содержимым страницы.
pub const PRESET_GAP: f64 = 100.0;

/// Выделенные дети одного родителя: родитель, число детей, позиции и узлы.
type SiblingGroup = (Option<NodeId>, usize, Vec<(usize, NodeId)>);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Arrange {
    Forward,
    Backward,
    Front,
    Back,
}

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
            C::Copy => {
                if let Some(bytes) = self.copy() {
                    self.out.events.push(EditorEvent::Clipboard(bytes));
                }
            }
            C::Cut => {
                if let Some(bytes) = self.copy() {
                    self.out.events.push(EditorEvent::Clipboard(bytes));
                    self.delete_selection();
                }
            }
            C::Paste => self.out.events.push(EditorEvent::PasteRequested),
            C::CopyAsPng => {
                let ids = self.ordered_selection();
                if !ids.is_empty() {
                    self.out.events.push(EditorEvent::ExportPng(ids));
                }
            }
            C::Duplicate => {
                let ids = top_level(&self.scene, &self.selection);
                if !ids.is_empty() {
                    self.ctx()
                        .commit(|tx, scene| duplicate(tx, scene, &ids, Vec2::ZERO));
                }
            }
            C::Delete => self.delete_selection(),
            C::SelectAll => self.select_all(),
            C::Escape => {
                if self.tool.kind() != ToolKind::Move {
                    self.set_tool(ToolKind::Move);
                } else {
                    self.selection.clear();
                }
            }
            C::Group => self.wrap(NodeKind::Group),
            C::FrameSelection => self.wrap(NodeKind::Frame),
            C::Ungroup => self.ungroup(),
            C::BringForward => self.arrange(Arrange::Forward),
            C::SendBackward => self.arrange(Arrange::Backward),
            C::BringToFront => self.arrange(Arrange::Front),
            C::SendToBack => self.arrange(Arrange::Back),
            C::NudgeLeft => self.nudge(Vec2::new(-1.0, 0.0)),
            C::NudgeRight => self.nudge(Vec2::new(1.0, 0.0)),
            C::NudgeUp => self.nudge(Vec2::new(0.0, -1.0)),
            C::NudgeDown => self.nudge(Vec2::new(0.0, 1.0)),
            C::NudgeLeftBig => self.nudge(Vec2::new(-10.0, 0.0)),
            C::NudgeRightBig => self.nudge(Vec2::new(10.0, 0.0)),
            C::NudgeUpBig => self.nudge(Vec2::new(0.0, -10.0)),
            C::NudgeDownBig => self.nudge(Vec2::new(0.0, 10.0)),
            C::ZoomToFit => {
                if let Some(b) = self.scene.bounds() {
                    self.camera.fit(b, FIT_PADDING_PX);
                }
            }
            C::ZoomToSelection => {
                let b = self
                    .selection
                    .iter()
                    .filter_map(|id| self.scene.slot(*id))
                    .filter_map(|s| world_box(&self.scene, s))
                    .reduce(|a, b| a.union(b));
                if let Some(b) = b {
                    self.camera.fit(b, FIT_PADDING_PX);
                }
            }
            C::Zoom100 => self.camera.set_zoom(1.0),
            C::ZoomIn => self.zoom_center(2.0),
            C::ZoomOut => self.zoom_center(0.5),
            C::ToggleVisible => self.toggle(
                |p| p.visible,
                |v| PropsPatch {
                    visible: Some(v),
                    ..Default::default()
                },
            ),
            C::ToggleLock => self.toggle(
                |p| !p.locked,
                |v| PropsPatch {
                    locked: Some(!v),
                    ..Default::default()
                },
            ),
            C::ToggleUi => self.out.events.push(EditorEvent::ToggleUi),
        }
    }

    fn zoom_center(&mut self, factor: f64) {
        let v = self.camera.viewport();
        self.camera
            .zoom_at(Point::new(v.width / 2.0, v.height / 2.0), factor);
    }

    /// Выделение верхнего уровня в порядке отрисовки, снизу вверх.
    fn ordered_selection(&self) -> Vec<NodeId> {
        let mut ids: Vec<(Vec<u32>, NodeId)> = top_level(&self.scene, &self.selection)
            .into_iter()
            .filter_map(|id| self.scene.slot(id).map(|s| (self.scene.order_key(s), id)))
            .collect();
        ids.sort_by(|a, b| a.0.cmp(&b.0));
        ids.into_iter().map(|(_, id)| id).collect()
    }

    fn delete_selection(&mut self) {
        let ids = top_level(&self.scene, &self.selection);
        if ids.is_empty() {
            return;
        }
        self.ctx().commit(|tx, _| {
            tx.delete_nodes(&ids)?;
            Ok(Vec::new())
        });
    }

    /// Все соседи в контексте выделения (или объекты верхнего уровня), видимые и
    /// не заблокированные.
    fn select_all(&mut self) {
        let parent = self
            .selection
            .first()
            .and_then(|id| self.scene.node(*id))
            .and_then(|n| n.parent);
        let list = match parent {
            Some(p) => self.scene.at(p).children.clone(),
            None => self.scene.roots().to_vec(),
        };
        self.selection = list
            .into_iter()
            .filter(|s| self.scene.is_visible(*s) && !locked(&self.scene, *s))
            .map(|s| self.scene.at(s).id)
            .filter(|id| !self.scene.is_draft(*id))
            .collect();
    }

    /// Ctrl+G и Ctrl+Alt+G: оборачивает выделение в группу или фрейм. Контейнер
    /// встаёт на место верхнего выделенного узла; узлы остаются на экране там же.
    fn wrap(&mut self, kind: NodeKind) {
        let ids = self.ordered_selection();
        let Some(&top) = ids.last() else {
            return;
        };
        let top_slot = self.scene.slot(top).expect("выделение в сцене");
        let parent_slot = self.scene.at(top_slot).parent;
        let parent = parent_slot.map(|p| self.scene.at(p).id);
        let pw = parent_world(&self.scene, top_slot);
        let index = self.scene.index(top_slot) as usize + 1;
        // Рамка фрейма — общая рамка узлов в координатах родителя.
        let inv = pw.inverse();
        let frame_box = ids
            .iter()
            .filter_map(|id| self.scene.node(*id))
            .filter_map(|n| {
                n.local_bounds
                    .map(|b| (inv * n.world).transform_rect_bbox(b))
            })
            .reduce(|a, b| a.union(b))
            .unwrap_or(Rect::ZERO);
        let origin = if kind == NodeKind::Frame {
            frame_box.origin()
        } else {
            Point::ZERO
        };
        self.ctx().commit(|tx, scene| {
            let c = tx.create_node(parent, Some(index), kind)?;
            let mut patch = PropsPatch {
                x: Some(origin.x),
                y: Some(origin.y),
                ..Default::default()
            };
            if kind == NodeKind::Frame {
                patch.w = Some(frame_box.width());
                patch.h = Some(frame_box.height());
            }
            tx.set_props(c, &patch)?;
            let c_world = pw * Affine::translate(origin.to_vec2());
            tx.move_nodes(&ids, Some(c), 0)?;
            for id in &ids {
                let n = scene.node(*id).expect("выделение в сцене");
                if n.parent == parent_slot {
                    // Тот же родитель: сдвиг точно, без ошибок округления матриц.
                    tx.set_props(
                        *id,
                        &PropsPatch {
                            x: Some(n.props.x - origin.x),
                            y: Some(n.props.y - origin.y),
                            ..Default::default()
                        },
                    )?;
                } else {
                    tx.set_props(*id, &reposition(c_world, n.world))?;
                }
            }
            Ok(vec![c])
        });
    }

    /// Ctrl+Shift+G: дети групп и фреймов выделения встают на их место у родителя.
    fn ungroup(&mut self) {
        let mut ids: Vec<(Vec<u32>, NodeId)> = top_level(&self.scene, &self.selection)
            .into_iter()
            .filter_map(|id| self.scene.slot(id).map(|s| (s, id)))
            .filter(|(s, _)| {
                let n = self.scene.at(*s);
                matches!(n.props.kind, NodeKind::Group | NodeKind::Frame) && !n.children.is_empty()
            })
            .map(|(s, id)| (self.scene.order_key(s), id))
            .collect();
        if ids.is_empty() {
            return;
        }
        // Сверху вниз: перенос в верхнее место не сдвигает позиции нижних.
        ids.sort_by(|a, b| b.0.cmp(&a.0));
        self.ctx().commit(|tx, scene| {
            let mut selection = Vec::new();
            for (_, g) in &ids {
                let slot = scene.slot(*g).expect("в сцене");
                let parent = scene.at(slot).parent.map(|p| scene.at(p).id);
                let pw = parent_world(scene, slot);
                let index = scene.index(slot) as usize;
                let children = tx.children(Some(*g));
                tx.move_nodes(&children, parent, index)?;
                for c in &children {
                    let world = scene.node(*c).expect("в сцене").world;
                    tx.set_props(*c, &reposition(pw, world))?;
                }
                tx.delete_nodes(&[*g])?;
                selection.extend(children);
            }
            Ok(selection)
        });
    }

    fn arrange(&mut self, op: Arrange) {
        let ids = self.ordered_selection();
        if ids.is_empty() {
            return;
        }
        // Узлы по родителям: позиция, число соседей.
        let mut groups: Vec<SiblingGroup> = Vec::new();
        for id in &ids {
            let s = self.scene.slot(*id).expect("в сцене");
            let parent_slot = self.scene.at(s).parent;
            let parent = parent_slot.map(|p| self.scene.at(p).id);
            let len = match parent_slot {
                Some(p) => self.scene.at(p).children.len(),
                None => self.scene.roots().len(),
            };
            let item = (self.scene.index(s) as usize, *id);
            match groups.iter_mut().find(|g| g.0 == parent) {
                Some(g) => g.2.push(item),
                None => groups.push((parent, len, vec![item])),
            }
        }
        let selection = self.selection.clone();
        self.ctx().commit(|tx, _| {
            for (parent, len, mut items) in groups {
                items.sort_by_key(|(i, _)| *i);
                let block: Vec<NodeId> = items.iter().map(|(_, id)| *id).collect();
                match op {
                    Arrange::Front => tx.move_nodes(&block, parent, usize::MAX)?,
                    Arrange::Back => tx.move_nodes(&block, parent, 0)?,
                    Arrange::Forward => {
                        // Сверху вниз: каждый выделенный меняется местами с соседом
                        // выше, если тот не выделен.
                        let mut taken: BTreeSet<usize> = items.iter().map(|(i, _)| *i).collect();
                        for (i, id) in items.iter().rev() {
                            if i + 1 < len && !taken.contains(&(i + 1)) {
                                tx.move_node(*id, parent, i + 1)?;
                                taken.remove(i);
                                taken.insert(i + 1);
                            }
                        }
                    }
                    Arrange::Backward => {
                        let mut taken: BTreeSet<usize> = items.iter().map(|(i, _)| *i).collect();
                        for (i, id) in items.iter() {
                            if *i > 0 && !taken.contains(&(i - 1)) {
                                tx.move_node(*id, parent, i - 1)?;
                                taken.remove(i);
                                taken.insert(i - 1);
                            }
                        }
                    }
                }
            }
            Ok(selection)
        });
    }

    /// Стрелки: сдвиг выделения на `d` экранных осей мира (1 или 10 px).
    fn nudge(&mut self, d: Vec2) {
        let ids: Vec<NodeId> = top_level(&self.scene, &self.selection)
            .into_iter()
            .filter(|id| {
                self.scene
                    .slot(*id)
                    .is_some_and(|s| !locked(&self.scene, s))
            })
            .collect();
        if ids.is_empty() {
            return;
        }
        let selection = self.selection.clone();
        self.ctx().commit(|tx, scene| {
            for id in &ids {
                let slot = scene.slot(*id).expect("в сцене");
                let dl = local_delta(parent_world(scene, slot), d);
                let p = tx.props(*id)?;
                tx.set_props(
                    *id,
                    &PropsPatch {
                        x: Some(p.x + dl.x),
                        y: Some(p.y + dl.y),
                        ..Default::default()
                    },
                )?;
            }
            Ok(selection)
        });
    }

    /// Ctrl+Shift+H и Ctrl+Shift+L: если хоть один узел в состоянии «да» —
    /// все переходят в «нет», иначе все в «да».
    fn toggle(&mut self, get: fn(&Props) -> bool, set: fn(bool) -> PropsPatch) {
        let ids = top_level(&self.scene, &self.selection);
        if ids.is_empty() {
            return;
        }
        let any = ids
            .iter()
            .filter_map(|id| self.scene.node(*id))
            .any(|n| get(&n.props));
        let patch = set(!any);
        let selection = self.selection.clone();
        self.ctx().commit(|tx, _| {
            for id in &ids {
                tx.set_props(*id, &patch)?;
            }
            Ok(selection)
        });
    }

    /// Копия выделения в формате буфера обмена Ferrum; `None` — нечего копировать.
    pub fn copy(&mut self) -> Option<Vec<u8>> {
        let ids = self.ordered_selection();
        if ids.is_empty() {
            return None;
        }
        let page = self.doc.page(self.page).ok()?;
        let mut nodes: Vec<ClipNode> = Vec::new();
        for id in &ids {
            let n = self.scene.node(*id)?;
            let mut props = page.props(*id).ok()?;
            // Корень копии — с мировым положением и поворотом.
            let pose = decompose(n.world, 1.0, 1.0);
            props.x = pose.x;
            props.y = pose.y;
            props.rotation = pose.rotation;
            let root = nodes.len();
            nodes.push(ClipNode {
                parent: None,
                props,
            });
            let mut stack = vec![(*id, root)];
            while let Some((src, at)) = stack.pop() {
                for child in page.children(Some(src)) {
                    let props = page.props(child).ok()?;
                    stack.push((child, nodes.len()));
                    nodes.push(ClipNode {
                        parent: Some(at),
                        props,
                    });
                }
            }
        }
        let mut hashes: Vec<String> = nodes
            .iter()
            .flat_map(|n| n.props.fills.iter().filter_map(|f| f.image_hash.clone()))
            .collect();
        hashes.sort();
        hashes.dedup();
        let images = hashes
            .into_iter()
            .filter_map(|h| {
                let bytes = self.doc.images.get(&h)?.to_vec();
                Some((h, bytes))
            })
            .collect();
        let bounds = ids
            .iter()
            .filter_map(|id| self.scene.slot(*id))
            .filter_map(|s| world_box(&self.scene, s))
            .reduce(|a, b| a.union(b))
            .unwrap_or(Rect::ZERO);
        Some(
            ClipPayload {
                nodes,
                images,
                bounds,
            }
            .encode(),
        )
    }

    /// Куда вставлять: в единственный выделенный фрейм, иначе к родителю
    /// выделения, иначе в корень. Родитель, его мировая матрица и рамка фрейма.
    fn paste_target(&self) -> (Option<NodeId>, Affine, Option<Rect>) {
        let frame_of = |id: NodeId| {
            let s = self.scene.slot(id)?;
            let n = self.scene.at(s);
            Some((Some(id), n.world, world_box(&self.scene, s)))
        };
        if let [one] = self.selection[..] {
            if self
                .scene
                .node(one)
                .is_some_and(|n| n.props.kind == NodeKind::Frame)
            {
                if let Some(t) = frame_of(one) {
                    return t;
                }
            }
        }
        let parent = self
            .selection
            .first()
            .and_then(|id| self.scene.node(*id))
            .and_then(|n| n.parent)
            .map(|p| self.scene.at(p).id);
        match parent.and_then(frame_of) {
            Some((id, world, b)) => {
                let is_frame = id
                    .and_then(|i| self.scene.node(i))
                    .is_some_and(|n| n.props.kind == NodeKind::Frame);
                (id, world, if is_frame { b } else { None })
            }
            None => (None, Affine::IDENTITY, None),
        }
    }

    /// Сдвиг вставки: на месте, если исходное место видно и помещается в цель,
    /// иначе — в центр цели (фрейма или видимой части холста).
    fn paste_offset(&self, bounds: Rect, target_box: Option<Rect>) -> Vec2 {
        let view = self.camera.visible_world_rect();
        let fits = target_box.is_none_or(|t| {
            t.contains(bounds.origin()) && t.contains(Point::new(bounds.x1, bounds.y1))
        });
        let visible = view.intersect(bounds).area() > 0.0 || view.contains(bounds.center());
        if fits && visible {
            Vec2::ZERO
        } else {
            target_box.unwrap_or(view).center() - bounds.center()
        }
    }

    /// Вставка данных Ferrum из буфера обмена одной транзакцией: узлы получают
    /// новые идентификаторы и выделяются. Данные не доверенные; при ошибке документ
    /// и история не меняются.
    pub fn paste(&mut self, bytes: &[u8]) -> Result<(), ClipboardError> {
        self.cancel_gesture();
        let payload = match ClipPayload::decode(bytes) {
            Ok(p) => p,
            Err(e) => {
                // Документ и история не тронуты: интерфейс узнаёт об ошибке из события.
                self.out.events.push(EditorEvent::Error(e.to_string()));
                self.emit();
                return Err(e);
            }
        };
        let (target, target_world, target_box) = self.paste_target();
        let offset = self.paste_offset(payload.bounds, target_box);
        let shift = Affine::translate(offset);
        let ok = self.ctx().commit(|tx, _| {
            let mut made: Vec<NodeId> = Vec::with_capacity(payload.nodes.len());
            let mut roots = Vec::new();
            for n in &payload.nodes {
                let mut props = n.props.clone();
                let parent = match n.parent {
                    Some(i) => Some(made[i]),
                    None => {
                        let world = shift * ferrum_scene::shape::local_transform(&props);
                        let pose = decompose(target_world.inverse() * world, 1.0, 1.0);
                        props.x = pose.x;
                        props.y = pose.y;
                        props.rotation = pose.rotation;
                        target
                    }
                };
                let id = tx.create_node(parent, None, props.kind)?;
                tx.set_props(id, &full_patch(&props))?;
                if n.parent.is_none() {
                    roots.push(id);
                }
                made.push(id);
            }
            Ok(roots)
        });
        if ok {
            // Байты кладутся только после успешной вставки: хэши уже проверены
            // `decode`, а неудавшаяся вставка не оставляет в документе ничего.
            for (hash, data) in &payload.images {
                if self.doc.images.get(hash).is_none() {
                    self.doc.images.add(data.clone());
                }
            }
        }
        self.emit();
        if ok {
            Ok(())
        } else {
            // Причину (`Error`) commit уже отправил в события.
            Err(ClipboardError::Corrupted("не удалось вставить узлы".into()))
        }
    }

    /// Изображение из системного буфера или перетащенного файла: встаёт в центр
    /// видимой части (или выделенного фрейма) в исходном размере.
    pub fn insert_image(&mut self, hash: &str, width: f64, height: f64) {
        self.cancel_gesture();
        let ok = |v: f64| v.is_finite() && v > 0.0;
        if !ok(width) || !ok(height) {
            self.out
                .events
                .push(EditorEvent::Error("У изображения нет размера".into()));
            self.emit();
            return;
        }
        let (target, target_world, target_box) = self.paste_target();
        let center = target_box.map_or(self.view_center(), |b| b.center());
        let origin = target_world.inverse() * (center - Vec2::new(width / 2.0, height / 2.0));
        let mut props = Props::defaults(NodeKind::Image);
        props.x = origin.x.round();
        props.y = origin.y.round();
        props.w = width;
        props.h = height;
        props.fills = vec![Fill::image(hash)];
        self.ctx().commit(|tx, _| {
            let id = tx.create_node(target, None, NodeKind::Image)?;
            tx.set_props(id, &full_patch(&props))?;
            Ok(vec![id])
        });
        self.emit();
    }

    /// Фрейм из пресета (раздел 7.5): справа от содержимого страницы, выделяется,
    /// камера переезжает к нему, если его не видно.
    pub fn add_frame_preset(&mut self, preset: &FramePreset) {
        self.cancel_gesture();
        let (x, y) = match self.scene.bounds() {
            Some(b) => ((b.x1 + PRESET_GAP).round(), b.y0.round()),
            None => {
                let c = self.view_center();
                (
                    (c.x - preset.width / 2.0).round(),
                    (c.y - preset.height / 2.0).round(),
                )
            }
        };
        let patch = PropsPatch {
            name: Some(preset.name.clone()),
            x: Some(x),
            y: Some(y),
            w: Some(preset.width),
            h: Some(preset.height),
            ..Default::default()
        };
        self.ctx().commit(|tx, _| {
            let id = tx.create_node(None, None, NodeKind::Frame)?;
            tx.set_props(id, &patch)?;
            Ok(vec![id])
        });
        let frame = Rect::new(x, y, x + preset.width, y + preset.height);
        if !self.camera.visible_world_rect().contains(frame.center()) {
            self.camera.set_center(frame.center());
        }
        self.emit();
    }
}
