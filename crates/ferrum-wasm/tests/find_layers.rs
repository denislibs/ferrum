//! Поиск слоёв и переход к слою в ядре worker'а (спека подсистемы 2, разделы 7
//! и 11): `FindLayers { query, limit }` → `LayersFound { query, items }` с
//! пределами запроса и числа; `RevealLayer { id }` выделяет слой, раскрывает
//! предков в панели и прокручивает её к строке.
#![cfg(not(target_arch = "wasm32"))]

use ferrum_editor::Platform;
use ferrum_wasm::core::Core;
use ferrum_wasm::protocol::*;
use kurbo::Size;

const VIEW: Size = Size::new(800.0, 600.0);

fn demo(nodes: u32) -> Core {
    let mut core = Core::new(Platform::Windows, VIEW, 1.0, Backend::Cpu);
    core.command(Command::Demo { nodes });
    core.take_events();
    core
}

fn found(core: &mut Core, query: &str, limit: u32) -> Vec<FoundLayerDto> {
    core.command(Command::FindLayers {
        query: query.into(),
        limit,
    });
    let ev = core.take_events();
    let mut answers = ev.iter().filter_map(|e| match e {
        CoreEvent::LayersFound { query: q, items } => Some((q.clone(), items.clone())),
        _ => None,
    });
    let (q, items) = answers.next().expect("ответ LayersFound");
    assert!(answers.next().is_none(), "один ответ на запрос");
    assert_eq!(q, query, "ответ несёт свой запрос");
    items
}

fn layer_rows(core: &mut Core) -> (u32, Vec<LayerRow>, Option<u32>) {
    core.command(Command::LayersWindow {
        start: 0,
        count: 200,
    });
    core.take_events()
        .into_iter()
        .rev()
        .find_map(|e| match e {
            CoreEvent::Layers {
                total,
                rows,
                reveal,
                ..
            } => Some((total, rows, reveal)),
            _ => None,
        })
        .expect("строки слоёв")
}

#[test]
fn find_layers_answers_with_its_query_best_first() {
    let mut core = demo(200);
    let items = found(&mut core, "экран 1", 50);
    assert_eq!(items[0].name, "Экран 1");
    assert_eq!(items[0].kind, NodeType::Frame);
    // Раскладка: «'rhfy» — «экран».
    let names: Vec<String> = found(&mut core, "'rhfy", 50)
        .into_iter()
        .map(|f| f.name)
        .collect();
    assert_eq!(names, ["Экран 1", "Экран 0"], "сверху вниз, как в панели");
    // Предел: сколько просили, но не больше 200.
    assert_eq!(found(&mut core, "слой", 7).len(), 7);
    assert_eq!(found(&mut core, "слой", 100_000).len(), 198);
    let mut big = demo(1000);
    assert_eq!(found(&mut big, "слой", u32::MAX).len(), 200);
    assert!(found(&mut core, "", 50).is_empty());
    assert!(found(&mut core, "нет такого", 50).is_empty());
}

#[test]
fn query_over_the_limit_is_an_error_without_an_answer() {
    let mut core = demo(100);
    // 129 букв кириллицы — 258 байт.
    core.command(Command::FindLayers {
        query: "я".repeat(129),
        limit: 50,
    });
    let ev = core.take_events();
    assert!(ev.iter().any(|e| matches!(e, CoreEvent::Error { .. })));
    assert!(!ev
        .iter()
        .any(|e| matches!(e, CoreEvent::LayersFound { .. })));
    // Ровно 256 байт — можно.
    assert!(found(&mut core, &"я".repeat(128), 50).is_empty());
}

#[test]
fn reveal_selects_expands_ancestors_and_scrolls_the_panel() {
    let mut core = demo(200);
    let (total, rows, _) = layer_rows(&mut core);
    assert_eq!(total, 2, "фреймы свёрнуты");
    // «Слой 5» второго фрейма («Экран 1» — верхний, первая строка).
    let target = found(&mut core, "слой 5", 1).remove(0);
    assert_eq!(target.name, "Слой 5");
    core.command(Command::RevealLayer {
        id: target.id.clone(),
    });
    let ev = core.take_events();
    assert!(ev.iter().any(
        |e| matches!(e, CoreEvent::Selection { ids, .. } if ids == std::slice::from_ref(&target.id))
    ));
    // Панель: предок раскрыт и прокручена к строке слоя.
    let (total, after, reveal) = ev
        .iter()
        .rev()
        .find_map(|e| match e {
            CoreEvent::Layers {
                total,
                rows,
                reveal,
                ..
            } => Some((*total, rows.clone(), *reveal)),
            _ => None,
        })
        .expect("строки слоёв после перехода");
    assert_eq!(total, 2 + 99);
    let at = reveal.expect("прокрутка к строке") as usize;
    assert_eq!(after[at].id, target.id);
    assert!(after[at].selected);
    assert!(rows.iter().any(|r| r.name == "Экран 1"));
    // Слой в виде.
    let b = core.editor().camera().visible_world_rect();
    let id = target.id.parse().unwrap();
    let n = core.editor().scene().node(id).unwrap();
    let world = n.world.transform_rect_bbox(n.local_bounds.unwrap());
    assert!(b.contains(world.origin()) && b.contains(world.origin() + world.size().to_vec2()));
}

#[test]
fn reveal_of_a_wrong_or_missing_layer_is_an_error() {
    let mut core = demo(100);
    for id in ["мусор", "999@999"] {
        core.command(Command::RevealLayer { id: id.into() });
        let ev = core.take_events();
        assert!(
            ev.iter().any(|e| matches!(e, CoreEvent::Error { .. })),
            "{id}: {ev:?}"
        );
        assert!(!ev.iter().any(|e| matches!(e, CoreEvent::Selection { .. })));
    }
}
