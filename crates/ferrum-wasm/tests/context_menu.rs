//! Контекстные меню в ядре worker'а (спека подсистемы 2, разделы 5 и 11):
//! `ContextMenu { target }` — выбор на правой кнопке и событие `ContextMenu`
//! последним в пачке, после выделения и состояний команд; неверная цель —
//! ошибка и пустое меню; «Развернуть всё» и «Дублировать страницу».
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

fn menu(ev: &[CoreEvent]) -> Vec<MenuItemDto> {
    match ev.last() {
        Some(CoreEvent::ContextMenu { items }) => items.clone(),
        other => panic!("последним должно быть меню: {other:?}"),
    }
}

fn find_sub<'a>(items: &'a [MenuItemDto], name: &str) -> &'a [MenuItemDto] {
    items
        .iter()
        .find_map(|n| match n {
            MenuItemDto::Submenu { title, items } if title == name => Some(items.as_slice()),
            _ => None,
        })
        .unwrap_or_else(|| panic!("нет «{name}»"))
}

fn layer_rows(core: &mut Core) -> (u32, Vec<LayerRow>, Option<u32>) {
    core.command(Command::LayersWindow {
        start: 0,
        count: 400,
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
fn canvas_menu_comes_after_selection_and_states() {
    let mut core = demo(200);
    // Мир (200, 40) — «Слой 1» в «Экран 0», на экране (140, 64).
    core.command(Command::ContextMenu {
        target: ContextTarget::Canvas { x: 140.0, y: 64.0 },
    });
    let ev = core.take_events();
    let at = |f: fn(&CoreEvent) -> bool| ev.iter().position(f).expect("событие");
    let sel = at(|e| matches!(e, CoreEvent::Selection { count: 1, .. }));
    let states = at(|e| matches!(e, CoreEvent::CommandStates { .. }));
    assert!(sel < ev.len() - 1 && states < ev.len() - 1);
    let items = menu(&ev);
    assert!(matches!(&items[0], MenuItemDto::Command { key } if key == "edit.copy"));
    let under = find_sub(&items, "Выделить слой");
    let names: Vec<&str> = under
        .iter()
        .map(|n| match n {
            MenuItemDto::Node { name, .. } => name.as_str(),
            other => panic!("{other:?}"),
        })
        .collect();
    assert_eq!(names, ["Слой 1", "Экран 0"]);
}

#[test]
fn wrong_targets_are_errors_with_an_empty_menu() {
    let mut core = demo(100);
    for target in [
        ContextTarget::Canvas {
            x: f64::NAN,
            y: 0.0,
        },
        ContextTarget::Layer {
            id: "мусор".into()
        },
        ContextTarget::Page { id: "1@1".into() },
    ] {
        core.command(Command::ContextMenu { target });
        let ev = core.take_events();
        assert!(
            ev.iter().any(|e| matches!(e, CoreEvent::Error { .. })),
            "{ev:?}"
        );
        assert!(menu(&ev).is_empty());
    }
}

#[test]
fn layer_menu_selects_like_the_panel_and_expands_all() {
    let mut core = demo(200);
    let (total, rows, _) = layer_rows(&mut core);
    assert_eq!(total, 2, "фреймы свёрнуты");
    core.command(Command::ContextMenu {
        target: ContextTarget::Layer {
            id: rows[0].id.clone(),
        },
    });
    let ev = core.take_events();
    assert!(ev
        .iter()
        .any(|e| matches!(e, CoreEvent::Selection { ids, .. } if ids == &[rows[0].id.clone()])));
    let items = menu(&ev);
    let actions: Vec<(MenuActionDto, String)> = items
        .iter()
        .filter_map(|n| match n {
            MenuItemDto::Action { action, title, .. } => Some((*action, title.clone())),
            _ => None,
        })
        .collect();
    assert_eq!(
        actions,
        [
            (MenuActionDto::RenameLayer, "Переименовать".into()),
            (MenuActionDto::ExpandAll, "Развернуть всё".into()),
            (MenuActionDto::CollapseAll, "Свернуть всё".into())
        ]
    );
    // Выбор из панели: строка не прокручивается к себе.
    core.command(Command::ExpandAllLayers { expanded: true });
    let (total, _, reveal) = layer_rows(&mut core);
    assert_eq!((total, reveal), (200, None));
    core.command(Command::ExpandAllLayers { expanded: false });
    assert_eq!(layer_rows(&mut core).0, 2);
}

#[test]
fn page_menu_and_duplicate_page() {
    let mut core = demo(100);
    core.command(Command::AddPage);
    let pages = |ev: &[CoreEvent]| {
        ev.iter()
            .rev()
            .find_map(|e| match e {
                CoreEvent::Pages { pages, current, .. } => Some((pages.clone(), current.clone())),
                _ => None,
            })
            .expect("страницы")
    };
    let (list, _) = pages(&core.take_events());
    let first = list[0].id.clone();
    core.command(Command::ContextMenu {
        target: ContextTarget::Page { id: first.clone() },
    });
    let items = menu(&core.take_events());
    assert!(items.iter().any(|n| matches!(
        n,
        MenuItemDto::Action {
            action: MenuActionDto::DeletePage,
            enabled: true,
            ..
        }
    )));
    core.command(Command::DuplicatePage { id: first });
    let ev = core.take_events();
    let (list, current) = pages(&ev);
    let names: Vec<&str> = list.iter().map(|p| p.name.as_str()).collect();
    assert_eq!(names, ["Страница 1", "Страница 1 (копия)", "Страница 2"]);
    assert_eq!(current, list[1].id);
    assert_eq!(layer_rows(&mut core).0, 1, "копия с фреймом оригинала");
    // Один шаг отмены: список страниц приходит и после отмены, и после повтора.
    core.take_events();
    core.command(Command::Execute {
        command: "edit.undo".into(),
    });
    let (list, current) = pages(&core.take_events());
    assert_eq!(list.len(), 2);
    assert_eq!(
        current, list[1].id,
        "назад на страницу, с которой дублировали"
    );
    core.command(Command::Execute {
        command: "edit.redo".into(),
    });
    let (list, current) = pages(&core.take_events());
    assert_eq!(list.len(), 3);
    assert_eq!(current, list[1].id);
}

#[test]
fn rulers_setting_goes_through_set_view_and_the_command_reports_back() {
    let mut core = demo(10);
    core.command(Command::SetView {
        rulers: Some(true),
        guides: None,
    });
    let ev = core.take_events();
    assert!(
        !ev.iter().any(|e| matches!(e, CoreEvent::View { .. })),
        "настройка пришла от интерфейса"
    );
    assert!(ev.iter().any(|e| matches!(
        e,
        CoreEvent::CommandStates { changed } if changed.iter().any(|s| s.key == "view.rulers" && s.checked == Some(true))
    )));
    core.command(Command::Execute {
        command: "view.rulers".into(),
    });
    let ev = core.take_events();
    assert!(ev.iter().any(|e| matches!(
        e,
        CoreEvent::View {
            rulers: false,
            guides: true
        }
    )));
}

#[test]
fn set_guides_preview_commit_and_errors() {
    let mut core = demo(10);
    let page = core.editor().document().pages()[0].0.to_string();
    let owner = GuideOwnerDto::Page { id: page };
    let list = vec![GuideDto {
        axis: GuideAxisDto::X,
        offset: 12.0,
    }];
    let errors = |ev: Vec<CoreEvent>| {
        ev.into_iter()
            .filter(|e| matches!(e, CoreEvent::Error { .. }))
            .count()
    };
    core.take_events();
    core.command(Command::SetGuides {
        owner: owner.clone(),
        guides: list.clone(),
        phase: EditPhase::Preview,
    });
    assert_eq!(errors(core.take_events()), 0);
    assert!(!core.editor().document().can_undo());
    core.command(Command::SetGuides {
        owner: owner.clone(),
        guides: list.clone(),
        phase: EditPhase::Commit,
    });
    let ev = core.take_events();
    assert!(
        ev.iter()
            .any(|e| matches!(e, CoreEvent::Modified { modified: true, .. })),
        "{ev:?}"
    );
    assert_eq!(errors(ev), 0);
    assert!(core.editor().document().can_undo());
    for bad in [
        vec![GuideDto {
            axis: GuideAxisDto::Y,
            offset: f64::NAN,
        }],
        vec![GuideDto {
            axis: GuideAxisDto::Y,
            offset: 2e9,
        }],
        vec![
            GuideDto {
                axis: GuideAxisDto::Y,
                offset: 0.0
            };
            1001
        ],
    ] {
        core.command(Command::SetGuides {
            owner: owner.clone(),
            guides: bad,
            phase: EditPhase::Commit,
        });
        assert_eq!(errors(core.take_events()), 1);
    }
    core.command(Command::SetGuides {
        owner: GuideOwnerDto::Page {
            id: "мусор".into()
        },
        guides: list.clone(),
        phase: EditPhase::Commit,
    });
    assert_eq!(errors(core.take_events()), 1);
    core.command(Command::SetGuides {
        owner: GuideOwnerDto::Frame { id: "1@1".into() },
        guides: list,
        phase: EditPhase::Commit,
    });
    assert_eq!(errors(core.take_events()), 1, "такого узла нет");
}

#[test]
fn pan_view_moves_the_camera_and_keeps_the_hover() {
    let mut core = demo(10);
    let x = core.editor().camera().center().x;
    core.command(Command::PanView { dx: 100.0 });
    assert_ne!(core.editor().camera().center().x, x);
    assert!(core
        .take_events()
        .iter()
        .all(|e| !matches!(e, CoreEvent::Error { .. })));
}
