//! Булевы операции над выделением (спека подсистемы 3, раздел 10): Alt+Shift+U,
//! S, I, E (Option+Shift на macOS, как в Figma) и выпадающий список тулбара. Выделение оборачивается булевой группой
//! на месте верхнего слоя одним шагом отмены; у выделенных булевых групп та же
//! команда меняет операцию.
//!
//! Как в Figma: группа берёт заливки и обводки верхнего слоя (у «Вычитания» —
//! нижнего, из которого вычитают), называется по операции, фреймы в булеву
//! группу не входят, нужно хотя бы два слоя.

use crate::editor::{Editor, EditorEvent};
use crate::geometry::parent_world;
use crate::ops::reposition;
use ferrum_doc::{BooleanOp, NodeId, NodeKind, Props, PropsPatch};

/// Слой, чей стиль берёт новая булева группа: у группы — её верхний (у
/// «Вычитания» — нижний) потомок со своими заливками.
fn style_source(editor: &Editor, id: NodeId, top: bool) -> Option<Props> {
    let n = editor.scene.node(id)?;
    if n.props.kind != NodeKind::Group {
        return Some(n.props.clone());
    }
    let mut kids: Vec<NodeId> = n.children.iter().map(|&c| editor.scene.at(c).id).collect();
    if top {
        kids.reverse();
    }
    kids.into_iter().find_map(|c| style_source(editor, c, top))
}

impl Editor {
    /// Булева операция `op` над выделением (одна команда — один шаг).
    pub(crate) fn boolean(&mut self, op: BooleanOp) {
        self.finish_vector_edit();
        let ids = self.ordered_selection();
        if ids.is_empty() {
            return;
        }
        // Выделены только булевы группы — у них меняется операция.
        let all_booleans = ids.iter().all(|id| {
            self.scene
                .node(*id)
                .is_some_and(|n| n.props.kind == NodeKind::Boolean)
        });
        if all_booleans {
            self.set_boolean_op(&ids, op);
            return;
        }
        let frames = ids.iter().any(|id| {
            self.scene
                .node(*id)
                .is_some_and(|n| n.props.kind == NodeKind::Frame)
        });
        let layers: Vec<NodeId> = ids
            .into_iter()
            .filter(|id| {
                self.scene
                    .node(*id)
                    .is_some_and(|n| n.props.kind != NodeKind::Frame)
            })
            .collect();
        if layers.len() < 2 {
            let msg = if frames {
                "Булевы операции не применяются к фреймам"
            } else {
                "Для булевой операции выделите хотя бы два слоя"
            };
            self.out.events.push(EditorEvent::Error(msg.into()));
            return;
        }
        if frames {
            self.out.events.push(EditorEvent::Error(
                "Фреймы в булеву группу не входят".into(),
            ));
        }
        let top = *layers.last().expect("слоёв не меньше двух");
        let top_slot = self.scene.slot(top).expect("выделение в сцене");
        let parent_slot = self.scene.at(top_slot).parent;
        let parent = parent_slot.map(|p| self.scene.at(p).id);
        let pw = parent_world(&self.scene, top_slot);
        let index = self.scene.index(top_slot) as usize + 1;
        let source = if op == BooleanOp::Subtract {
            style_source(self, layers[0], false)
        } else {
            style_source(self, top, true)
        };
        let mut props = Props {
            name: op.name().into(),
            boolean_op: Some(op),
            ..Props::defaults(NodeKind::Boolean)
        };
        if let Some(s) = source {
            props.fills = s.fills;
            props.strokes = s.strokes;
            props.stroke_style = s.stroke_style;
        }
        self.ctx().commit(|tx, scene| {
            let b = tx.create_with_props(parent, Some(index), &props)?;
            tx.move_nodes(&layers, Some(b), 0)?;
            for id in &layers {
                let n = scene.node(*id).expect("выделение в сцене");
                // Тот же родитель: координаты те же (группа в начале координат).
                if n.parent != parent_slot {
                    tx.set_props(*id, &reposition(pw, n.world, &n.props))?;
                }
            }
            Ok(vec![b])
        });
    }

    /// Меняет операцию булевых групп `ids`; имя по умолчанию («Объединение» и
    /// т. п.) меняется вместе с ней, своё — остаётся.
    fn set_boolean_op(&mut self, ids: &[NodeId], op: BooleanOp) {
        let patches: Vec<(NodeId, PropsPatch)> = ids
            .iter()
            .filter_map(|id| {
                let p = &self.scene.node(*id)?.props;
                if p.boolean_op == Some(op) {
                    return None;
                }
                let default_name = BooleanOp::ALL.iter().any(|o| o.name() == p.name);
                Some((
                    *id,
                    PropsPatch {
                        boolean_op: Some(op),
                        name: default_name.then(|| op.name().to_string()),
                        ..Default::default()
                    },
                ))
            })
            .collect();
        if patches.is_empty() {
            return;
        }
        let selection = ids.to_vec();
        self.ctx().commit(|tx, _| {
            for (id, patch) in &patches {
                tx.set_props(*id, patch)?;
            }
            Ok(selection)
        });
    }
}
