//! Чужие правки в редакторе (спека подсистемы 4, раздел 8): сцена совпадает с документом, выделение
//! теряет удалённые узлы, жест над удалённым узлом отменяется, чужие свойства жест не прерывают,
//! каретка едет за чужими вставками, отмена не трогает чужого, и всё это не делает документ изменённым.

mod common;

use std::sync::Arc;

use common::text::loaded_fonts;
use common::*;
use ferrum_doc::container::{decode, encode_full};
use ferrum_doc::*;
use ferrum_editor::*;
use ferrum_render::Camera;
use kurbo::Point;

fn open(bytes: &[u8]) -> H {
    let mut ed = Editor::new(
        decode(bytes).unwrap().document,
        None,
        VIEW,
        1.0,
        Platform::Windows,
    )
    .unwrap();
    let mut cam = Camera::new(VIEW, 1.0);
    cam.set_center(Point::new(VIEW.width / 2.0, VIEW.height / 2.0));
    ed.set_camera(cam);
    ed.set_fonts(loaded_fonts());
    ed.sync_start();
    for (k, v) in ed.take_sync_subscribes() {
        ed.sync_subscribed(k, v);
    }
    ed.take_events();
    H { ed, t: 1000.0 }
}

fn pair() -> (H, H) {
    let bytes = encode_full(&Document::new()).unwrap();
    (open(&bytes), open(&bytes))
}

fn deliver(from: &mut H, to: &mut H) -> Vec<RemoteEffects> {
    for (k, v) in to.ed.take_sync_subscribes() {
        to.ed.sync_subscribed(k, v);
    }
    let mut out = Vec::new();
    for b in from.ed.take_sync_updates(MAX_SYNC_BATCH_BYTES).unwrap() {
        out.push(to.ed.apply_remote(b.doc, &[&b.bytes]).unwrap());
        from.ed.sync_acked(b.doc, &b.end);
    }
    out
}

#[test]
fn foreign_edits_update_scene_and_are_not_own_edits() {
    let (mut a, mut b) = pair();
    let id = a.rect(None, 10.0, 10.0, 50.0, 50.0);
    b.ed.take_events();
    deliver(&mut a, &mut b);
    let events = b.ed.take_events();
    assert!(events.contains(&EditorEvent::RemoteChanged), "{events:?}");
    assert!(
        !events.contains(&EditorEvent::DocumentChanged),
        "«изменён» чужая правка не ставит"
    );
    assert!(b.ed.scene().node(id).is_some());
    b.assert_scene_clean();
    assert!(!b.ed.document().can_undo(), "чужая правка не шаг отмены");
    a.set(id, patch(100.0, 10.0, 50.0, 50.0));
    deliver(&mut a, &mut b);
    assert_eq!(b.props(id).x, 100.0);
    b.assert_scene_clean();
}

#[test]
fn deleted_by_other_leaves_selection_silently() {
    let (mut a, mut b) = pair();
    let x = a.rect(None, 10.0, 10.0, 50.0, 50.0);
    let y = a.rect(None, 100.0, 10.0, 50.0, 50.0);
    deliver(&mut a, &mut b);
    b.ed.select(&[x, y]);
    a.ed.transact(|tx| {
        tx.delete_nodes(&[x])?;
        Ok(vec![])
    })
    .unwrap();
    b.ed.take_events();
    deliver(&mut a, &mut b);
    assert_eq!(b.sel(), vec![y]);
    let events = b.ed.take_events();
    assert!(
        events
            .iter()
            .all(|e| !matches!(e, EditorEvent::Error(_) | EditorEvent::Notice(_))),
        "без сообщения: {events:?}"
    );
    b.assert_scene_clean();
}

#[test]
fn move_to_other_parent_keeps_selection() {
    let (mut a, mut b) = pair();
    let f = a.frame(None, 300.0, 300.0, 200.0, 200.0);
    let x = a.rect(None, 10.0, 10.0, 50.0, 50.0);
    deliver(&mut a, &mut b);
    b.ed.select(&[x]);
    a.ed.transact(|tx| {
        tx.move_node(x, Some(f), 0)?;
        Ok(vec![])
    })
    .unwrap();
    deliver(&mut a, &mut b);
    assert_eq!(b.sel(), vec![x]);
    assert_eq!(b.parent(x), Some(f));
    b.assert_scene_clean();
}

#[test]
fn drag_cancelled_when_dragged_node_deleted_by_other() {
    let (mut a, mut b) = pair();
    let x = a.rect(None, 100.0, 100.0, 50.0, 50.0);
    deliver(&mut a, &mut b);
    let steps = b.ed.step_count();
    b.drag_hold((120.0, 120.0), (220.0, 120.0), none());
    assert!(b.ed.is_busy());
    assert!(b.ed.scene().has_preview());
    a.ed.transact(|tx| {
        tx.delete_nodes(&[x])?;
        Ok(vec![])
    })
    .unwrap();
    deliver(&mut a, &mut b);
    assert!(!b.ed.is_busy(), "жест отменён");
    b.up(220.0, 120.0, none());
    assert_eq!(b.ed.step_count(), steps, "шага нет");
    assert!(b.sel().is_empty());
    b.assert_scene_clean();
}

#[test]
fn foreign_props_do_not_interrupt_drag_and_last_write_wins_on_release() {
    let (mut a, mut b) = pair();
    let x = a.rect(None, 100.0, 100.0, 50.0, 50.0);
    deliver(&mut a, &mut b);
    b.drag_hold((120.0, 120.0), (220.0, 120.0), none());
    a.set(
        x,
        PropsPatch {
            y: Some(400.0),
            name: Some("чужое".into()),
            ..Default::default()
        },
    );
    deliver(&mut a, &mut b);
    assert!(b.ed.is_busy(), "жест продолжается");
    b.mv(240.0, 120.0, none());
    b.up(240.0, 120.0, none());
    assert_eq!(b.props(x).name, "чужое");
    assert_eq!(b.props(x).x, 220.0);
    deliver(&mut b, &mut a);
    assert_eq!(a.props(x), b.props(x), "сошлись");
    a.assert_scene_clean();
    b.assert_scene_clean();
}

#[test]
fn drag_in_progress_survives_foreign_create_and_delete_elsewhere() {
    let (mut a, mut b) = pair();
    let x = a.rect(None, 100.0, 100.0, 50.0, 50.0);
    let other = a.rect(None, 500.0, 500.0, 50.0, 50.0);
    deliver(&mut a, &mut b);
    b.drag_hold((120.0, 120.0), (220.0, 120.0), none());
    a.rect(None, 600.0, 600.0, 20.0, 20.0);
    a.ed.transact(|tx| {
        tx.delete_nodes(&[other])?;
        Ok(vec![])
    })
    .unwrap();
    deliver(&mut a, &mut b);
    assert!(b.ed.is_busy());
    b.up(220.0, 120.0, none());
    assert_eq!(b.props(x).x, 200.0);
    b.assert_scene_clean();
}

#[test]
fn caret_follows_foreign_insert_and_session_ends_when_text_deleted() {
    let (mut a, mut b) = pair();
    let id = a.add(None, NodeKind::Text, patch(100.0, 100.0, 300.0, 100.0));
    a.ed.transact(|tx| {
        tx.insert_text(id, 0, "абвгд", None)?;
        Ok(vec![id])
    })
    .unwrap();
    deliver(&mut a, &mut b);
    b.ed.select(&[id]);
    b.key("Enter", none());
    b.key("ArrowLeft", none());
    for _ in 0..3 {
        b.key("ArrowRight", none());
    }
    assert_eq!(b.ed.text_edit_info().unwrap().focus, 3);
    a.ed.transact(|tx| {
        tx.insert_text(id, 1, "XY", None)?;
        Ok(vec![id])
    })
    .unwrap();
    deliver(&mut a, &mut b);
    let info = b.ed.text_edit_info().unwrap();
    assert_eq!(
        (info.anchor, info.focus),
        (5, 5),
        "курсор — за теми же символами"
    );
    a.ed.transact(|tx| {
        tx.insert_text(id, 7, "Z", None)?;
        Ok(vec![id])
    })
    .unwrap();
    deliver(&mut a, &mut b);
    assert_eq!(
        b.ed.text_edit_info().unwrap().focus,
        5,
        "правка после курсора его не двигает"
    );
    a.ed.transact(|tx| {
        tx.delete_nodes(&[id])?;
        Ok(vec![])
    })
    .unwrap();
    deliver(&mut a, &mut b);
    assert!(b.ed.text_edit_info().is_none(), "сеанс правки закрыт");
    b.assert_scene_clean();
}

#[test]
fn vector_session_closes_with_notice_when_other_changes_network() {
    let (mut a, mut b) = pair();
    let square = VectorNetwork {
        points: vec![
            VectorPoint::new(0.0, 0.0),
            VectorPoint::new(100.0, 0.0),
            VectorPoint::new(100.0, 100.0),
        ],
        segments: (0..2).map(|i| VectorSegment::line(i, i + 1)).collect(),
        regions: vec![],
    };
    let id = a.add(
        None,
        NodeKind::Vector,
        PropsPatch {
            vector: Some(Arc::new(square.clone())),
            ..patch(100.0, 100.0, 100.0, 100.0)
        },
    );
    deliver(&mut a, &mut b);
    assert!(b.ed.start_vector_edit(id));
    assert!(b.ed.is_editing_vector());
    // Чужая правка другого свойства сеанс не трогает.
    a.set(
        id,
        PropsPatch {
            name: Some("имя".into()),
            ..Default::default()
        },
    );
    deliver(&mut a, &mut b);
    assert!(b.ed.is_editing_vector());
    let mut bigger = square;
    bigger.points.push(VectorPoint::new(0.0, 100.0));
    a.set(
        id,
        PropsPatch {
            vector: Some(Arc::new(bigger.clone())),
            ..Default::default()
        },
    );
    b.ed.take_events();
    deliver(&mut a, &mut b);
    assert!(!b.ed.is_editing_vector(), "сеанс закрыт");
    let events = b.ed.take_events();
    assert!(
        events.contains(&EditorEvent::Notice(VECTOR_CHANGED_BY_OTHER)),
        "{events:?}"
    );
    assert_eq!(b.props(id).vector.as_deref(), Some(&bigger));
    b.assert_scene_clean();
}

#[test]
fn page_removed_by_other_moves_to_first_and_new_page_is_listed() {
    let (mut a, mut b) = pair();
    let added = a.ed.add_page("Вторая").unwrap();
    deliver(&mut a, &mut b);
    assert_eq!(b.ed.document().pages().len(), 2);
    b.ed.take_events();
    b.ed.set_page(added).unwrap();
    let kept = b.ed.document().pages()[0].0;
    a.ed.document_mut_for_tests().delete_page(added).unwrap();
    b.ed.take_events();
    deliver(&mut a, &mut b);
    assert_eq!(b.ed.page(), kept);
    let events = b.ed.take_events();
    assert!(events.contains(&EditorEvent::PageChanged(kept)));
    assert!(
        events.contains(&EditorEvent::Notice(PAGE_REMOVED_BY_OTHER)),
        "{events:?}"
    );
}

#[test]
fn undo_affects_only_own_edits_and_skips_deleted() {
    let (mut a, mut b) = pair();
    let own = a.rect(None, 10.0, 10.0, 50.0, 50.0);
    let shared = a.rect(None, 100.0, 10.0, 50.0, 50.0);
    deliver(&mut a, &mut b);
    let foreign = b.rect(None, 200.0, 10.0, 50.0, 50.0);
    deliver(&mut b, &mut a);
    a.set(shared, patch(101.0, 10.0, 50.0, 50.0));
    deliver(&mut a, &mut b);
    b.ed.transact(|tx| {
        tx.delete_nodes(&[shared])?;
        Ok(vec![])
    })
    .unwrap();
    deliver(&mut b, &mut a);
    // Верхний шаг a — про `shared`, его удалил b: пропуск, следующий шаг — создание `shared`, его тоже нет,
    // дальше — создание `own`.
    a.ed.execute(CommandId::Undo);
    assert!(a.ed.scene().node(own).is_none(), "свой узел отменён");
    assert!(a.ed.scene().node(foreign).is_some(), "чужой на месте");
    a.assert_scene_clean();
    deliver(&mut a, &mut b);
    assert!(b.ed.scene().node(own).is_none());
    assert_eq!(a.state(), b.state());
}

#[test]
fn scene_rebuilt_after_foreign_unknown_node_and_goes_read_only() {
    let (mut a, mut b) = pair();
    let x = a.rect(None, 10.0, 10.0, 50.0, 50.0);
    deliver(&mut a, &mut b);
    b.ed.select(&[x]);
    let page = a.ed.page();
    a.ed.document_mut_for_tests()
        .page(page)
        .unwrap()
        .force_kind_for_tests(x, "star");
    let fx = deliver(&mut a, &mut b);
    assert!(fx.iter().any(|f| f.rebuilt && f.became_read_only), "{fx:?}");
    assert!(b.sel().is_empty());
    b.assert_scene_clean();
}

#[test]
fn missing_image_of_foreign_fill_is_requested() {
    let (mut a, mut b) = pair();
    let x = a.rect(None, 10.0, 10.0, 50.0, 50.0);
    deliver(&mut a, &mut b);
    let hash = a.ed.add_image(b"png".to_vec());
    assert_eq!(a.ed.take_sync_images().len(), 1, "байты для сервера");
    assert!(a.ed.take_sync_images().is_empty());
    a.set(
        x,
        PropsPatch {
            fills: Some(vec![Fill::image(hash.clone())]),
            ..Default::default()
        },
    );
    let fx = deliver(&mut a, &mut b);
    assert!(
        fx.iter().any(|f| f.missing_images == vec![hash.clone()]),
        "{fx:?}"
    );
}

fn no_error(h: &mut H) {
    let events = h.ed.take_events();
    assert!(
        events.iter().all(|e| !matches!(e, EditorEvent::Error(_))),
        "без технической ошибки: {events:?}"
    );
}

fn delete_node(h: &mut H, id: NodeId) {
    h.ed.transact(|tx| {
        tx.delete_nodes(&[id])?;
        Ok(vec![])
    })
    .unwrap();
}

fn x_edit(value: f64) -> PanelEdit {
    PanelEdit::Number {
        field: NumField::X,
        value,
        relative: false,
    }
}

#[test]
fn panel_edit_of_node_deleted_by_other_is_cancelled_silently() {
    let (mut a, mut b) = pair();
    let x = a.rect(None, 10.0, 10.0, 50.0, 50.0);
    deliver(&mut a, &mut b);
    b.ed.select(&[x]);
    let steps = b.ed.step_count();
    b.ed.edit_selection(&x_edit(40.0), Phase::Preview);
    assert!(b.ed.scene().has_preview());
    delete_node(&mut a, x);
    b.ed.take_events();
    deliver(&mut a, &mut b);
    assert!(!b.ed.scene().has_preview(), "жест панели снят");
    b.ed.edit_selection(&x_edit(40.0), Phase::Commit);
    no_error(&mut b);
    assert_eq!(b.ed.step_count(), steps, "шага нет");
    b.assert_scene_clean();
}

#[test]
fn panel_edit_goes_on_for_other_targets_when_one_is_deleted() {
    let (mut a, mut b) = pair();
    let x = a.rect(None, 10.0, 10.0, 50.0, 50.0);
    let y = a.rect(None, 100.0, 10.0, 50.0, 50.0);
    deliver(&mut a, &mut b);
    b.ed.select(&[x, y]);
    b.ed.edit_nodes(&x_edit(40.0), Phase::Preview, &[x, y]);
    delete_node(&mut a, x);
    b.ed.take_events();
    deliver(&mut a, &mut b);
    b.ed.edit_nodes(&x_edit(40.0), Phase::Commit, &[x, y]);
    no_error(&mut b);
    assert_eq!(b.props(y).x, 40.0, "выжившая цель получила правку");
    b.assert_scene_clean();
}

#[test]
fn panel_edit_of_group_with_child_deleted_by_other_has_no_error() {
    let (mut a, mut b) = pair();
    let g = a.frame(None, 0.0, 0.0, 200.0, 200.0);
    let kid = a.rect(Some(g), 10.0, 10.0, 50.0, 50.0);
    deliver(&mut a, &mut b);
    b.ed.select(&[g]);
    let scale = PanelEdit::Scale {
        factor: 2.0,
        anchor: ferrum_editor::scale::Anchor::TOP_LEFT,
    };
    b.ed.edit_selection(&scale, Phase::Preview);
    delete_node(&mut a, kid);
    b.ed.take_events();
    deliver(&mut a, &mut b);
    b.ed.edit_selection(&scale, Phase::Commit);
    no_error(&mut b);
    b.assert_scene_clean();
}

#[test]
fn drawing_inside_frame_deleted_by_other_is_cancelled_silently() {
    let (mut a, mut b) = pair();
    let f = a.frame(None, 100.0, 100.0, 400.0, 400.0);
    deliver(&mut a, &mut b);
    let steps = b.ed.step_count();
    b.ed.set_tool(ToolKind::Rect);
    b.drag_hold((150.0, 150.0), (250.0, 250.0), none());
    assert!(b.ed.scene().has_preview());
    delete_node(&mut a, f);
    b.ed.take_events();
    deliver(&mut a, &mut b);
    assert!(!b.ed.is_busy(), "жест снят");
    b.up(250.0, 250.0, none());
    no_error(&mut b);
    assert_eq!(b.ed.step_count(), steps);
    b.assert_scene_clean();
}

#[test]
fn text_box_inside_frame_deleted_by_other_is_cancelled_silently() {
    let (mut a, mut b) = pair();
    let f = a.frame(None, 100.0, 100.0, 400.0, 400.0);
    deliver(&mut a, &mut b);
    b.ed.set_tool(ToolKind::Text);
    b.drag_hold((150.0, 150.0), (250.0, 250.0), none());
    delete_node(&mut a, f);
    b.ed.take_events();
    deliver(&mut a, &mut b);
    assert!(!b.ed.is_busy(), "жест снят");
    b.up(250.0, 250.0, none());
    no_error(&mut b);
    b.assert_scene_clean();
}

#[test]
fn drawing_in_other_frame_survives_deletion_elsewhere() {
    let (mut a, mut b) = pair();
    let f = a.frame(None, 100.0, 100.0, 300.0, 300.0);
    let other = a.frame(None, 500.0, 100.0, 300.0, 300.0);
    deliver(&mut a, &mut b);
    b.ed.set_tool(ToolKind::Rect);
    b.drag_hold((150.0, 150.0), (250.0, 250.0), none());
    delete_node(&mut a, other);
    deliver(&mut a, &mut b);
    assert!(b.ed.is_busy(), "другой фрейм жесту не мешает");
    b.up(250.0, 250.0, none());
    assert_eq!(b.children(Some(f)).len(), 1);
    b.assert_scene_clean();
}

fn triangle(h: &mut H) -> NodeId {
    let net = VectorNetwork {
        points: vec![
            VectorPoint::new(0.0, 0.0),
            VectorPoint::new(100.0, 0.0),
            VectorPoint::new(100.0, 100.0),
        ],
        segments: (0..2).map(|i| VectorSegment::line(i, i + 1)).collect(),
        regions: vec![],
    };
    h.add(
        None,
        NodeKind::Vector,
        PropsPatch {
            vector: Some(Arc::new(net)),
            ..patch(100.0, 100.0, 100.0, 100.0)
        },
    )
}

#[test]
fn unwritten_pen_path_on_vector_deleted_by_other_closes_with_notice() {
    let (mut a, mut b) = pair();
    let id = triangle(&mut a);
    deliver(&mut a, &mut b);
    assert!(b.ed.start_vector_edit(id));
    b.ed.set_tool(ToolKind::Pen);
    b.click(400.0, 400.0, none());
    b.click(500.0, 400.0, none());
    delete_node(&mut a, id);
    b.ed.take_events();
    deliver(&mut a, &mut b);
    assert!(!b.ed.is_editing_vector(), "сеанс закрыт");
    let events = b.ed.take_events();
    assert!(
        events.contains(&EditorEvent::Notice(VECTOR_DELETED_BY_OTHER)),
        "{events:?}"
    );
    assert!(events.iter().all(|e| !matches!(e, EditorEvent::Error(_))));
    b.ed.end_vector_edit();
    no_error(&mut b);
    b.assert_scene_clean();
}

#[test]
fn vector_session_without_unwritten_work_closes_silently_when_deleted() {
    let (mut a, mut b) = pair();
    let id = triangle(&mut a);
    deliver(&mut a, &mut b);
    assert!(b.ed.start_vector_edit(id));
    delete_node(&mut a, id);
    b.ed.take_events();
    deliver(&mut a, &mut b);
    assert!(!b.ed.is_editing_vector());
    let events = b.ed.take_events();
    assert!(
        events
            .iter()
            .all(|e| !matches!(e, EditorEvent::Error(_) | EditorEvent::Notice(_))),
        "{events:?}"
    );
    b.assert_scene_clean();
}

#[test]
fn new_pen_path_inside_frame_deleted_by_other_closes_with_notice() {
    let (mut a, mut b) = pair();
    let f = a.frame(None, 100.0, 100.0, 600.0, 600.0);
    deliver(&mut a, &mut b);
    b.ed.set_tool(ToolKind::Pen);
    b.click(200.0, 200.0, none());
    b.click(300.0, 200.0, none());
    assert!(b.ed.is_editing_vector());
    delete_node(&mut a, f);
    b.ed.take_events();
    deliver(&mut a, &mut b);
    assert!(!b.ed.is_editing_vector(), "сеанс закрыт");
    let events = b.ed.take_events();
    assert!(
        events.contains(&EditorEvent::Notice(VECTOR_DELETED_BY_OTHER)),
        "{events:?}"
    );
    assert!(events.iter().all(|e| !matches!(e, EditorEvent::Error(_))));
    b.assert_scene_clean();
}

/// На странице два живых узла с одним `NodeId`: дальше каждая чужая правка идёт сборкой заново.
fn with_twins(a: &mut H, b: &mut H) {
    let id = a.rect(None, 700.0, 700.0, 10.0, 10.0);
    deliver(a, b);
    delete_node(a, id);
    a.ed.execute(CommandId::Undo);
    // Часы b уходят вперёд: его перенос побеждает удаление независимо от случайных пиров.
    let spare = b.rect(None, 720.0, 700.0, 10.0, 10.0);
    for x in 0..8 {
        b.set(spare, patch(720.0 + x as f64, 700.0, 10.0, 10.0));
    }
    b.ed.transact(|tx| {
        tx.move_node(id, None, 5)?;
        Ok(vec![])
    })
    .unwrap();
    deliver(a, b);
    deliver(b, a);
    deliver(a, b);
    deliver(b, a);
}

#[test]
fn vector_session_closes_when_rebuild_brings_new_network() {
    let (mut a, mut b) = pair();
    let id = triangle(&mut a);
    deliver(&mut a, &mut b);
    with_twins(&mut a, &mut b);
    assert!(b.ed.start_vector_edit(id));
    let mut bigger = (*b.props(id).vector.unwrap()).clone();
    bigger.points.push(VectorPoint::new(0.0, 100.0));
    a.set(
        id,
        PropsPatch {
            vector: Some(Arc::new(bigger.clone())),
            ..Default::default()
        },
    );
    b.ed.take_events();
    let fx = deliver(&mut a, &mut b);
    assert!(fx.iter().any(|f| f.rebuilt), "{fx:?}");
    assert!(!b.ed.is_editing_vector(), "сеанс закрыт");
    let events = b.ed.take_events();
    assert!(
        events.contains(&EditorEvent::Notice(VECTOR_CHANGED_BY_OTHER)),
        "{events:?}"
    );
    assert_eq!(b.props(id).vector.as_deref(), Some(&bigger));
    b.assert_scene_clean();
}

#[test]
fn vector_session_survives_rebuild_with_same_network() {
    let (mut a, mut b) = pair();
    let id = triangle(&mut a);
    deliver(&mut a, &mut b);
    with_twins(&mut a, &mut b);
    assert!(b.ed.start_vector_edit(id));
    a.set(
        id,
        PropsPatch {
            name: Some("имя".into()),
            ..Default::default()
        },
    );
    let fx = deliver(&mut a, &mut b);
    assert!(fx.iter().any(|f| f.rebuilt), "{fx:?}");
    assert!(b.ed.is_editing_vector(), "сеть та же: сеанс идёт");
}

#[test]
fn rebuild_keeps_drag_of_living_node_and_its_preview() {
    let (mut a, mut b) = pair();
    let x = a.rect(None, 100.0, 100.0, 50.0, 50.0);
    let other = a.rect(None, 500.0, 500.0, 50.0, 50.0);
    deliver(&mut a, &mut b);
    with_twins(&mut a, &mut b);
    b.drag_hold((120.0, 120.0), (220.0, 120.0), none());
    let moved = b.ed.scene().node(x).unwrap().props.x;
    assert_eq!(moved, 200.0);
    a.set(other, patch(510.0, 500.0, 50.0, 50.0));
    let fx = deliver(&mut a, &mut b);
    assert!(fx.iter().any(|f| f.rebuilt), "{fx:?}");
    assert!(b.ed.is_busy(), "сборка заново жест не прерывает");
    assert_eq!(
        b.ed.scene().node(x).unwrap().props.x,
        moved,
        "превью на месте"
    );
    b.mv(240.0, 120.0, none());
    b.up(240.0, 120.0, none());
    assert_eq!(b.props(x).x, 220.0);
    b.assert_scene_clean();
}

#[test]
fn rebuild_cancels_drag_of_deleted_node() {
    let (mut a, mut b) = pair();
    let x = a.rect(None, 100.0, 100.0, 50.0, 50.0);
    deliver(&mut a, &mut b);
    with_twins(&mut a, &mut b);
    b.drag_hold((120.0, 120.0), (220.0, 120.0), none());
    delete_node(&mut a, x);
    b.ed.take_events();
    let fx = deliver(&mut a, &mut b);
    assert!(fx.iter().any(|f| f.rebuilt), "{fx:?}");
    assert!(!b.ed.is_busy());
    b.up(220.0, 120.0, none());
    no_error(&mut b);
    assert!(b.sel().is_empty());
    b.assert_scene_clean();
}

#[test]
fn grouping_by_other_does_not_touch_drag_and_scene_matches() {
    let (mut a, mut b) = pair();
    let x = a.rect(None, 100.0, 100.0, 50.0, 50.0);
    let p = a.rect(None, 300.0, 300.0, 50.0, 50.0);
    let q = a.rect(None, 400.0, 300.0, 50.0, 50.0);
    deliver(&mut a, &mut b);
    b.drag_hold((120.0, 120.0), (220.0, 120.0), none());
    let f =
        a.ed.transact(|tx| {
            let f = tx.create_node(None, None, NodeKind::Frame)?;
            tx.move_node(p, Some(f), 0)?;
            tx.move_node(q, Some(f), 1)?;
            Ok(vec![f])
        })
        .unwrap()[0];
    let fx = deliver(&mut a, &mut b);
    assert!(fx.iter().all(|e| !e.rebuilt), "{fx:?}");
    assert!(b.ed.is_busy(), "жест продолжается");
    assert_eq!(b.parent(q), Some(f));
    b.up(220.0, 120.0, none());
    assert_eq!(b.props(x).x, 200.0);
    b.assert_scene_clean();
}

#[test]
fn going_read_only_cancels_drag() {
    let (mut a, mut b) = pair();
    let x = a.rect(None, 100.0, 100.0, 50.0, 50.0);
    let other = a.rect(None, 500.0, 500.0, 50.0, 50.0);
    deliver(&mut a, &mut b);
    b.drag_hold((120.0, 120.0), (220.0, 120.0), none());
    let page = a.ed.page();
    a.ed.document_mut_for_tests()
        .page(page)
        .unwrap()
        .force_kind_for_tests(other, "star");
    b.ed.take_events();
    deliver(&mut a, &mut b);
    assert!(!b.ed.is_busy(), "писать уже нельзя");
    b.up(220.0, 120.0, none());
    no_error(&mut b);
    assert_eq!(b.props(x).x, 100.0);
    b.assert_scene_clean();
}

#[test]
fn guide_over_frame_deleted_by_other_is_cancelled_silently() {
    let (mut a, mut b) = pair();
    let f = a.frame(None, 100.0, 100.0, 300.0, 300.0);
    deliver(&mut a, &mut b);
    b.ed.set_view(Some(true), Some(true));
    b.drag_hold((10.0, 170.0), (170.0, 170.0), none());
    assert!(b.ed.overlay().active_guide.is_some());
    delete_node(&mut a, f);
    b.ed.take_events();
    deliver(&mut a, &mut b);
    assert!(b.ed.overlay().active_guide.is_none(), "жест снят");
    b.up(170.0, 170.0, none());
    no_error(&mut b);
    assert!(b.ed.document().page_guides(b.ed.page()).is_empty());
}
