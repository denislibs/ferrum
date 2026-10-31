//! «Сплющить» (Ctrl/Cmd+E, спека подсистемы 3, раздел 9): выделенные векторы и
//! фигуры (и такие же узлы в выделенных группах) — один вектор одним шагом
//! отмены. Один узел сохраняет свою позу (поворот, отражение), несколько —
//! сливаются в координатах родителя верхнего из них; вектор встаёт на место
//! верхнего и берёт его свойства (имя, заливки, обводки), а области других
//! узлов сохраняют свои заливки цветом. Группа с заблокированным слоем не
//! сплющивается; скрытые слои группы в вектор не попадают (как в Figma).

use crate::editor::{Editor, EditorEvent};
use crate::geometry::parent_world;
use crate::selection::locked;
use crate::vector_edit::{as_vector, convert_to_vector};
use crate::vector_ops::{merge_networks, reframe, shape_network};
use ferrum_doc::{FillKind, NodeId, NodeKind, Props};

/// Сплющивается ли узел этого типа.
fn flattenable(k: NodeKind) -> bool {
    matches!(
        k,
        NodeKind::Vector | NodeKind::Rect | NodeKind::Ellipse | NodeKind::Line
    )
}

impl Editor {
    /// Сплющивает выделение в один вектор.
    pub(crate) fn flatten(&mut self) {
        self.finish_vector_edit();
        let ids = self.ordered_selection();
        // Узлы-источники снизу вверх: выделенные фигуры и фигуры в выделенных группах.
        let mut leaves: Vec<NodeId> = Vec::new();
        let mut removed: Vec<NodeId> = Vec::new();
        for id in &ids {
            let Some(slot) = self.scene.slot(*id) else {
                continue;
            };
            // Заблокированный и скрытый (сам или предком) не трогаются.
            if locked(&self.scene, slot) || !self.scene.is_visible(slot) {
                continue;
            }
            let kind = self.scene.at(slot).props.kind;
            if flattenable(kind) {
                leaves.push(*id);
                removed.push(*id);
            } else if kind == NodeKind::Group {
                let mut inner = Vec::new();
                let mut stack = vec![slot];
                let mut foreign = false;
                while let Some(s) = stack.pop() {
                    let n = self.scene.at(s);
                    // Заблокированный слой нельзя ни слить, ни удалить с группой —
                    // такую группу целиком не сплющить.
                    if n.props.locked {
                        foreign = true;
                        continue;
                    }
                    // Скрытые слои в результат не попадают (как в Figma) и
                    // уходят вместе с группой.
                    if !n.props.visible {
                        continue;
                    }
                    match n.props.kind {
                        NodeKind::Group => stack.extend(n.children.iter().rev()),
                        k if flattenable(k) => inner.push((self.scene.order_key(s), n.id)),
                        _ => foreign = true,
                    }
                }
                // Группу с текстом, изображением или фреймом сплющить целиком нельзя:
                // их пришлось бы потерять вместе с группой.
                if foreign || inner.is_empty() {
                    continue;
                }
                inner.sort_by(|a, b| a.0.cmp(&b.0));
                leaves.extend(inner.into_iter().map(|(_, id)| id));
                removed.push(*id);
            }
        }
        if leaves.is_empty() {
            self.out.events.push(EditorEvent::Error(
                "Нечего сплющить: сплющиваются векторы, прямоугольники, эллипсы, линии и их группы"
                    .into(),
            ));
            return;
        }
        let top = *removed.last().expect("есть узлы");
        let top_slot = self.scene.slot(top).expect("в сцене");
        // Один узел — вектор той же позы на его месте.
        if let ([one], [only]) = (&leaves[..], &removed[..]) {
            if one == only {
                let n = self.scene.at(top_slot);
                let props = n.props.clone();
                let Some(net) = shape_network(&props) else {
                    return;
                };
                if props.kind == NodeKind::Vector {
                    return;
                }
                let patch = reframe(&props, net);
                let id = *one;
                self.ctx()
                    .commit(|tx, _| Ok(vec![convert_to_vector(tx, id, &patch)?]));
                return;
            }
        }
        // Несколько — в координатах родителя верхнего узла.
        let pw = parent_world(&self.scene, top_slot);
        let inv = pw.inverse();
        let top_leaf = self
            .scene
            .node(*leaves.last().expect("есть узлы"))
            .expect("в сцене")
            .props
            .clone();
        let mut parts = Vec::new();
        for id in &leaves {
            let n = self.scene.node(*id).expect("в сцене");
            let Some(mut net) = shape_network(&n.props) else {
                continue;
            };
            // Заливки узла, отличные от заливок результата, остаются у его областей.
            let own = n.props.fills != top_leaf.fills
                && n.props.fills.iter().all(|f| f.kind == FillKind::Solid);
            for r in &mut net.regions {
                if r.fills.is_none() && own {
                    r.fills = Some(n.props.fills.clone());
                }
            }
            parts.push((net, inv * n.world));
        }
        let merged = merge_networks(&parts);
        let mut base = Props {
            x: 0.0,
            y: 0.0,
            w: 0.0,
            h: 0.0,
            rotation: 0.0,
            flip_x: false,
            flip_y: false,
            ..top_leaf
        };
        as_vector(&mut base);
        base.name = self.scene.at(top_slot).props.name.clone();
        let patch = reframe(&base, merged);
        patch.apply_to(&mut base);
        let (parent, index) = (
            self.scene.at(top_slot).parent.map(|p| self.scene.at(p).id),
            self.scene.index(top_slot) as usize + 1,
        );
        self.ctx().commit(|tx, _| {
            let v = tx.create_with_props(parent, Some(index), &base)?;
            tx.delete_nodes(&removed)?;
            Ok(vec![v])
        });
    }
}
