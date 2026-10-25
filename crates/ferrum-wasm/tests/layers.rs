//! Дерево слоёв в worker'е: окно строк ограничено, свёрнутый фрейм — одна строка,
//! раскрытие, поиск с предками, диапазон для Shift, раскрытие предков выделения
//! с холста; одинаковое окно второй раз не отправляется.
#![cfg(not(target_arch = "wasm32"))]

use std::collections::HashSet;

use ferrum_doc::{fixture, Document, NodeId};
use ferrum_scene::Scene;
use ferrum_wasm::layers::{Layers, NodeType, MAX_LAYER_WINDOW};

/// Страница-пример: фреймы «Экран N» по 99 фигур «Слой i».
fn page(nodes: usize) -> (Scene, Vec<NodeId>) {
    let mut doc = Document::new();
    let p = doc.pages()[0].0;
    let mut ids = vec![];
    doc.transact(p, vec![], |tx| {
        ids = fixture::fill(tx, nodes)?;
        Ok(vec![])
    })
    .unwrap();
    (Scene::from_page(doc.page(p).unwrap()), ids)
}

fn none() -> HashSet<NodeId> {
    HashSet::new()
}

#[test]
fn collapsed_frames_are_one_row_each_top_first() {
    let (scene, _) = page(1000);
    let mut l = Layers::new();
    l.set_window(0, 5);
    let w = l.window(&scene, &none()).unwrap();
    assert_eq!(w.total, 10, "10 фреймов, дети свёрнуты");
    assert_eq!(w.rows.len(), 5);
    assert_eq!(w.rows[0].name, "Экран 9", "верхний — первым");
    assert_eq!((w.rows[0].kind, w.rows[0].depth), (NodeType::Frame, 0));
    assert!(w.rows[0].has_children && !w.rows[0].expanded);
    assert!(
        l.window(&scene, &none()).is_none(),
        "то же окно не повторяется"
    );
    l.resend();
    assert!(l.window(&scene, &none()).is_some());
}

#[test]
fn window_is_capped_and_clamped() {
    let (scene, ids) = page(1000);
    let mut l = Layers::new();
    l.set_expanded(ids[0], true);
    l.set_window(0, 10_000);
    let w = l.window(&scene, &none()).unwrap();
    assert_eq!(w.total, 10 + 99);
    assert!(w.rows.len() as u32 <= MAX_LAYER_WINDOW);
    l.set_window(5000, 20);
    let w = l.window(&scene, &none()).unwrap();
    assert_eq!(
        (w.start, w.rows.len()),
        (109, 0),
        "начало за концом — пустое окно"
    );
}

#[test]
fn expanded_frame_lists_children_top_first() {
    let (scene, ids) = page(200);
    let mut l = Layers::new();
    // ids[0] — «Экран 0», нижний фрейм; его строка — вторая.
    l.set_expanded(ids[0], true);
    l.set_window(0, 4);
    let w = l.window(&scene, &none()).unwrap();
    let names: Vec<&str> = w.rows.iter().map(|r| r.name.as_str()).collect();
    assert_eq!(names, ["Экран 1", "Экран 0", "Слой 98", "Слой 97"]);
    assert_eq!(w.rows[2].depth, 1);
    let mut sel = HashSet::new();
    sel.insert(ids[0]);
    let w = l.window(&scene, &sel).unwrap();
    assert!(w.rows[1].selected && w.rows[2].parent_selected && !w.rows[2].selected);
}

#[test]
fn search_shows_matches_with_ancestors() {
    let (scene, _) = page(300);
    let mut l = Layers::new();
    l.set_filter("  слой 42 ");
    l.set_window(0, 50);
    let w = l.window(&scene, &none()).unwrap();
    // В каждом из трёх фреймов «Слой 42» и его фрейм.
    assert_eq!(w.total, 6);
    assert_eq!(w.rows[0].name, "Экран 2");
    assert!(w.rows[0].expanded, "при поиске всё раскрыто");
    assert_eq!((w.rows[1].name.as_str(), w.rows[1].depth), ("Слой 42", 1));
    l.set_filter("");
    assert_eq!(l.window(&scene, &none()).unwrap().total, 3);
}

#[test]
fn range_and_reveal() {
    let (scene, ids) = page(200);
    let mut l = Layers::new();
    // Выделение с холста внутри «Экран 0»: фрейм раскрывается, строка — 2-я снизу
    // вверх по фрейму.
    let child = ids[5];
    let at = l.reveal(&scene, &[child]).unwrap();
    l.set_window(at, 1);
    let w = l.window(&scene, &none()).unwrap();
    assert_eq!(w.rows[0].id, child.to_string());
    // От «Экран 1» (первая строка) до «Слой 0» (нижний ребёнок «Экран 0»).
    let rows = l.range(&scene, ids[100], ids[1]);
    assert_eq!(rows.len(), 1 + 1 + 99, "оба фрейма и дети раскрытого");
    assert_eq!((rows[0], rows[100]), (ids[100], ids[1]));
    assert_eq!(
        l.range(&scene, ids[1], ids[100]),
        rows,
        "в любом направлении"
    );
    let missing = ids[150];
    assert_eq!(
        l.range(&scene, missing, ids[0]),
        vec![ids[0]],
        "свёрнутый — только цель"
    );
}

#[test]
fn rows_follow_frames_not_nodes() {
    let (scene, ids) = page(30_000);
    let mut l = Layers::new();
    l.set_expanded(ids[0], true);
    l.set_window(0, 40);
    let w = l.window(&scene, &none()).unwrap();
    assert_eq!(w.total, 300 + 99);
    assert_eq!(w.rows.len(), 40);
    // Строк — по числу фреймов и раскрытых детей, не по числу узлов.
    assert_eq!(w.rows[1].name, "Экран 298");
}

// ---- Раунд правок 1: кэш строк сам устаревает, поиск наращивается ----

use ferrum_doc::{Changes, DocResult, NodeKind, PageId, Props, PropsPatch, Tx};
use ferrum_scene::Draft;

/// Документ и сцена, как их ведёт редактор: правка → изменения → `apply`.
struct World {
    doc: Document,
    page: PageId,
    scene: Scene,
    ids: Vec<NodeId>,
}

fn world(nodes: usize) -> World {
    let mut doc = Document::new();
    let page = doc.pages()[0].0;
    let mut ids = vec![];
    doc.transact(page, vec![], |tx| {
        ids = fixture::fill(tx, nodes)?;
        Ok(vec![])
    })
    .unwrap();
    doc.take_changes(page);
    doc.clear_history();
    let scene = Scene::from_page(doc.page(page).unwrap());
    World {
        doc,
        page,
        scene,
        ids,
    }
}

impl World {
    fn edit(&mut self, f: impl FnOnce(&Tx) -> DocResult<Vec<NodeId>>) {
        self.doc.transact(self.page, vec![], f).unwrap();
        self.sync();
    }

    fn sync(&mut self) -> Changes {
        let c = self.doc.take_changes(self.page);
        self.scene.apply(&c, self.doc.page(self.page).unwrap());
        c
    }

    /// Правка, о которой дереву слоёв сообщено, как это делает worker.
    fn edit_noted(&mut self, l: &mut Layers, f: impl FnOnce(&Tx) -> DocResult<Vec<NodeId>>) {
        self.doc.transact(self.page, vec![], f).unwrap();
        let c = self.sync();
        l.note_changes(&c, &self.scene);
    }
}

fn names(l: &mut Layers, w: &World, start: u32, count: u32) -> Vec<String> {
    l.set_window(start, count);
    l.resend();
    l.window(&w.scene, &none())
        .unwrap()
        .rows
        .into_iter()
        .map(|r| r.name)
        .collect()
}

fn rename(id: NodeId, name: &str) -> impl FnOnce(&Tx) -> DocResult<Vec<NodeId>> + '_ {
    move |tx| {
        tx.set_props(
            id,
            &PropsPatch {
                name: Some(name.into()),
                ..Default::default()
            },
        )?;
        Ok(vec![])
    }
}

#[test]
fn delete_without_invalidate_does_not_show_dead_rows() {
    let mut w = world(200);
    let mut l = Layers::new();
    assert_eq!(names(&mut l, &w, 0, 5), ["Экран 1", "Экран 0"]);
    let victim = w.ids[100];
    w.edit(|tx| {
        tx.delete_nodes(&[victim])?;
        Ok(vec![])
    });
    assert_eq!(names(&mut l, &w, 0, 5), ["Экран 0"]);
    assert_eq!(l.total(&w.scene), 1);
}

#[test]
fn move_and_rename_without_invalidate_follow_the_scene() {
    let mut w = world(200);
    let mut l = Layers::new();
    l.set_expanded(w.ids[0], true);
    assert_eq!(names(&mut l, &w, 0, 3), ["Экран 1", "Экран 0", "Слой 98"]);
    let (f0, f1, kid) = (w.ids[0], w.ids[100], w.ids[5]);
    // Ребёнок «Слой 4» уходит из «Экран 0» в «Экран 1» (свёрнут) и обратно не виден.
    w.edit(|tx| {
        tx.move_node(kid, Some(f1), 0)?;
        Ok(vec![])
    });
    assert_eq!(l.total(&w.scene), 2 + 98);
    w.edit(rename(f0, "Главная"));
    assert_eq!(names(&mut l, &w, 0, 2), ["Экран 1", "Главная"]);
}

#[test]
fn rename_while_searching_and_undo() {
    let mut w = world(200);
    let mut l = Layers::new();
    l.set_filter("слой 42");
    assert_eq!(l.total(&w.scene), 4);
    let kid = w.ids[1 + 42];
    assert_eq!(w.scene.node(kid).unwrap().props.name, "Слой 42");
    w.edit(rename(kid, "Кнопка"));
    assert_eq!(l.total(&w.scene), 2, "переименованный выпал из поиска");
    w.doc.undo().unwrap();
    w.sync();
    assert_eq!(l.total(&w.scene), 4, "отмена вернула совпадение");
    let n = names(&mut l, &w, 0, 10);
    assert_eq!(n, ["Экран 1", "Слой 42", "Экран 0", "Слой 42"]);
    l.set_filter("кнопк");
    assert_eq!(l.total(&w.scene), 0);
}

#[test]
fn freed_slot_reused_by_new_node_is_not_aliased() {
    let mut w = world(200);
    let mut l = Layers::new();
    assert_eq!(names(&mut l, &w, 0, 5), ["Экран 1", "Экран 0"]);
    let old = w.ids[100];
    w.edit(|tx| {
        tx.delete_nodes(&[old])?;
        Ok(vec![])
    });
    let mut fresh = None;
    w.edit(|tx| {
        let n = tx.create_node(None, None, NodeKind::Frame)?;
        tx.set_props(
            n,
            &PropsPatch {
                name: Some("Новый".into()),
                ..Default::default()
            },
        )?;
        fresh = Some(n);
        Ok(vec![])
    });
    let rows = {
        l.set_window(0, 5);
        l.resend();
        l.window(&w.scene, &none()).unwrap().rows
    };
    let got: Vec<&str> = rows.iter().map(|r| r.name.as_str()).collect();
    assert_eq!(got, ["Новый", "Экран 0"]);
    assert_eq!(rows[0].id, fresh.unwrap().to_string());
}

#[test]
fn stale_rows_after_scene_replacement_do_not_panic() {
    let w = world(200);
    let mut l = Layers::new();
    assert_eq!(names(&mut l, &w, 0, 5).len(), 2);
    // Сцену подменили целиком (другая страница): слоты чужие, метка другая.
    let other = world(30);
    l.set_window(0, 5);
    l.resend();
    let got = l.window(&other.scene, &none()).unwrap();
    assert_eq!(got.total, 1);
    assert_eq!(got.rows[0].name, "Экран 0");
}

fn draft(n: u32, parent: Option<NodeId>, name: &str) -> Draft {
    let mut props = Props::defaults(NodeKind::Rect);
    props.name = name.into();
    Draft {
        id: format!("{n}@{}", u64::MAX).parse().unwrap(),
        parent,
        props,
    }
}

#[test]
fn drafts_never_show_in_rows_or_search_and_gesture_end_is_safe() {
    let mut w = world(200);
    let mut l = Layers::new();
    l.set_expanded(w.ids[0], true);
    let f0 = w.ids[0];
    w.scene.set_drafts(&[
        draft(0, Some(f0), "Слой 42 копия"),
        draft(1, None, "Слой 42 корень"),
    ]);
    assert_eq!(l.total(&w.scene), 2 + 99, "черновики не строки");
    l.set_filter("слой 42");
    assert_eq!(l.total(&w.scene), 4, "и не результаты поиска");
    let before = names(&mut l, &w, 0, 20);
    assert!(before
        .iter()
        .all(|n| !n.contains("копия") && !n.contains("корень")));
    // Жест закончился (Esc): черновики ушли, строки остаются верными.
    w.scene.clear_preview();
    assert_eq!(names(&mut l, &w, 0, 20), before);
    // Жест закончился фиксацией: узел документа занимает освободившийся слот.
    w.scene.set_drafts(&[draft(2, Some(f0), "Слой 42 дубль")]);
    l.set_filter("слой 42");
    w.scene.clear_preview();
    w.edit(|tx| {
        let n = tx.create_node(Some(f0), None, NodeKind::Rect)?;
        tx.set_props(
            n,
            &PropsPatch {
                name: Some("Слой 42 дубль".into()),
                ..Default::default()
            },
        )?;
        Ok(vec![])
    });
    assert_eq!(l.total(&w.scene), 5);
    assert_eq!(names(&mut l, &w, 0, 20)[3], "Слой 42 дубль");
}

#[test]
fn expanded_while_searching_needs_a_kept_child() {
    let w = world(200);
    let mut l = Layers::new();
    // «Экран 1» совпадает сам, детей среди совпавших нет.
    l.set_filter("экран 1");
    l.set_window(0, 5);
    let got = l.window(&w.scene, &none()).unwrap();
    assert_eq!(got.total, 1);
    assert!(got.rows[0].has_children && !got.rows[0].expanded);
    l.set_filter("слой 42");
    l.set_window(0, 5);
    assert!(l.window(&w.scene, &none()).unwrap().rows[0].expanded);
}

#[test]
fn filter_is_clipped_before_trim_and_narrowing_matches_fresh_search() {
    let w = world(300);
    let mut l = Layers::new();
    let long = format!("{}{}", " ".repeat(200), "слой");
    l.set_filter(&long);
    assert!(l.filter().chars().count() <= 128);
    // Печатаем «слой 4» по букве: каждый шаг сужает предыдущий результат.
    let mut typed = String::new();
    for ch in "слой 4".chars() {
        typed.push(ch);
        l.set_filter(&typed);
        let mut fresh = Layers::new();
        fresh.set_filter(&typed);
        assert_eq!(l.total(&w.scene), fresh.total(&w.scene), "{typed:?}");
    }
    // Стереть букву — не сужение: результат должен вырасти.
    let narrow = l.total(&w.scene);
    l.set_filter("слой ");
    assert!(l.total(&w.scene) > narrow);
}

#[test]
fn row_flags_hidden_locked_and_ancestors() {
    let mut w = world(200);
    let (f0, kid) = (w.ids[0], w.ids[3]);
    w.edit(|tx| {
        tx.set_props(
            f0,
            &PropsPatch {
                visible: Some(false),
                locked: Some(true),
                ..Default::default()
            },
        )?;
        tx.set_props(
            kid,
            &PropsPatch {
                locked: Some(true),
                ..Default::default()
            },
        )?;
        Ok(vec![])
    });
    let mut l = Layers::new();
    l.set_expanded(f0, true);
    l.set_window(1, 99);
    let rows = l.window(&w.scene, &none()).unwrap().rows;
    assert!(!rows[0].visible && rows[0].locked);
    assert!(!rows[0].ancestor_hidden && !rows[0].ancestor_locked);
    let r = rows.iter().find(|r| r.id == kid.to_string()).unwrap();
    assert!(r.visible && r.locked && r.ancestor_hidden && r.ancestor_locked);
    let other = rows.iter().find(|r| r.id == w.ids[4].to_string()).unwrap();
    assert!(!other.locked && other.ancestor_locked);
}

#[test]
fn reveal_keeps_sibling_frames_collapsed() {
    let (scene, ids) = page(300);
    let mut l = Layers::new();
    let at = l.reveal(&scene, &[ids[5]]).unwrap();
    assert_eq!(l.total(&scene), 3 + 99);
    l.set_window(0, 200);
    let w = l.window(&scene, &none()).unwrap();
    let open: Vec<&str> = w
        .rows
        .iter()
        .filter(|r| r.expanded)
        .map(|r| r.name.as_str())
        .collect();
    assert_eq!(open, ["Экран 0"]);
    assert_eq!(w.rows[at as usize].id, ids[5].to_string());
}

#[test]
fn window_edges_and_reset() {
    let (scene, ids) = page(200);
    let mut l = Layers::new();
    l.set_window(u32::MAX, u32::MAX);
    let w = l.window(&scene, &none()).unwrap();
    assert_eq!((w.start, w.total, w.rows.len()), (2, 2, 0));
    l.set_window(0, 0);
    let w = l.window(&scene, &none()).unwrap();
    assert!(w.rows.is_empty());
    l.set_window(0, 10);
    l.set_expanded(ids[0], true);
    l.set_filter("слой");
    l.set_anchor(Some(ids[0]));
    l.reset();
    assert_eq!((l.filter(), l.anchor()), ("", None));
    assert_eq!(l.total(&scene), 2, "раскрытие сброшено");
    assert!(
        l.window(&scene, &none()).is_some(),
        "после reset окно уходит заново"
    );
}

#[test]
fn noted_edits_update_search_matches() {
    let mut w = world(200);
    let mut l = Layers::new();
    l.set_filter("слой 42");
    assert_eq!(l.total(&w.scene), 4);
    let kid = w.ids[1 + 42];
    w.edit_noted(&mut l, rename(kid, "Кнопка"));
    assert_eq!(l.total(&w.scene), 2);
    // Новый узел с подходящим именем появляется в результатах.
    let f1 = w.ids[100];
    w.edit_noted(&mut l, |tx| {
        let n = tx.create_node(Some(f1), None, NodeKind::Rect)?;
        tx.set_props(
            n,
            &PropsPatch {
                name: Some("Слой 42 новый".into()),
                ..Default::default()
            },
        )?;
        Ok(vec![])
    });
    assert_eq!(l.total(&w.scene), 3);
    // Удалённый — уходит, слот не мешает.
    w.edit_noted(&mut l, |tx| {
        tx.delete_nodes(&[f1])?;
        Ok(vec![])
    });
    assert_eq!(l.total(&w.scene), 0);
    w.edit_noted(&mut l, rename(w.ids[1 + 42], "Слой 42"));
    assert_eq!(names(&mut l, &w, 0, 10), ["Экран 0", "Слой 42"]);
    // Перенос не трогает имена.
    let (k, f0) = (w.ids[1 + 42], w.ids[0]);
    w.edit_noted(&mut l, |tx| {
        tx.move_node(k, Some(f0), 0)?;
        Ok(vec![])
    });
    assert_eq!(names(&mut l, &w, 0, 10), ["Экран 0", "Слой 42"]);
}

#[test]
fn skipped_note_changes_still_shows_fresh_names() {
    let mut w = world(200);
    let mut l = Layers::new();
    l.set_filter("слой 42");
    assert_eq!(l.total(&w.scene), 4);
    let kid = w.ids[1 + 42];
    w.edit(rename(kid, "Кнопка"));
    assert_eq!(l.total(&w.scene), 2, "без note_changes кэш имён не врёт");
    w.edit(rename(kid, "Слой 42"));
    assert_eq!(l.total(&w.scene), 4);
    // note_changes только для последней из двух правок: первая потеряна.
    let other = w.ids[1 + 7];
    w.edit(rename(other, "Слой 42 а"));
    w.edit_noted(&mut l, rename(kid, "Слой 42 б"));
    assert_eq!(l.total(&w.scene), 5);
    // Превью имени (без Changes) тоже не оставляет старое.
    let p = PropsPatch {
        name: Some("Другое".into()),
        ..Default::default()
    };
    w.scene.set_preview(&[(kid, p)]);
    assert_eq!(l.total(&w.scene), 4);
}

#[test]
fn scene_switch_resets_name_cache() {
    let a = world(200);
    let b = world(30);
    let mut l = Layers::new();
    l.set_filter("слой 28");
    assert_eq!(l.total(&a.scene), 4);
    assert_eq!(l.total(&b.scene), 2, "другая сцена — кэш имён чужой");
    l.prepare_search(&a.scene);
    assert_eq!(l.total(&a.scene), 4);
    // Сцена, пересобранная внутри apply, — новый экземпляр.
    assert_ne!(a.scene.generation(), b.scene.generation());
}
