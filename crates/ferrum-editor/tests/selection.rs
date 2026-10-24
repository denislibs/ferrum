//! Правила выделения на холсте (раздел 7.2 спеки).

mod common;

use common::*;
use ferrum_doc::{NodeId, NodeKind, PropsPatch};
use ferrum_editor::*;

struct Page {
    f: NodeId,
    g: NodeId,
    a: NodeId,
    b: NodeId,
    r: NodeId,
    s: NodeId,
    rg: NodeId,
    c: NodeId,
}

/// Фрейм F (100, 100, 400×300) с группой G (a, b) и прямоугольником r; в корне —
/// прямоугольник s и группа RG с прямоугольником c.
fn page(h: &mut H) -> Page {
    let f = h.frame(None, 100.0, 100.0, 400.0, 300.0);
    let g = h.add(Some(f), NodeKind::Group, PropsPatch::default());
    let a = h.rect(Some(g), 20.0, 20.0, 40.0, 40.0);
    let b = h.rect(Some(g), 80.0, 20.0, 40.0, 40.0);
    let r = h.rect(Some(f), 200.0, 150.0, 50.0, 50.0);
    let s = h.rect(None, 600.0, 100.0, 50.0, 50.0);
    let rg = h.add(None, NodeKind::Group, PropsPatch::default());
    let c = h.rect(Some(rg), 700.0, 300.0, 40.0, 40.0);
    Page {
        f,
        g,
        a,
        b,
        r,
        s,
        rg,
        c,
    }
}

#[test]
fn click_selects_top_level_object_in_its_context() {
    let mut h = H::new();
    let p = page(&mut h);
    // Ребёнок фрейма верхнего уровня — группа целиком, не её прямоугольник.
    h.click(130.0, 130.0, none());
    assert_eq!(h.sel(), vec![p.g]);
    h.click(320.0, 270.0, none());
    assert_eq!(h.sel(), vec![p.r]);
    // Объект в корне и ребёнок корневой группы — сам объект и группа.
    h.click(620.0, 120.0, none());
    assert_eq!(h.sel(), vec![p.s]);
    h.click(720.0, 320.0, none());
    assert_eq!(h.sel(), vec![p.rg]);
    // Пустое место фрейма с детьми не выбирает фрейм и снимает выделение.
    h.click(450.0, 350.0, none());
    assert!(h.sel().is_empty());
    // Фрейм выбирается кликом по названию над ним.
    h.click(150.0, 92.0, none());
    assert_eq!(h.sel(), vec![p.f]);
    // Пустой холст снимает выделение.
    h.click(900.0, 700.0, none());
    assert!(h.sel().is_empty());
    assert_eq!(h.ed.step_count(), 8, "выделение не пишет в историю");
}

#[test]
fn double_click_goes_deeper_and_context_sticks() {
    let mut h = H::new();
    let p = page(&mut h);
    h.dbl(130.0, 130.0);
    assert_eq!(h.sel(), vec![p.a], "двойной клик — на уровень глубже");
    // Сосед внутри той же группы выбирается обычным кликом.
    h.click(200.0, 140.0, none());
    assert_eq!(h.sel(), vec![p.b]);
    // Объект вне группы — на своём уровне по умолчанию.
    h.click(320.0, 270.0, none());
    assert_eq!(h.sel(), vec![p.r]);
    // Ctrl+клик — самый глубокий объект сразу.
    h.click(130.0, 130.0, ctrl());
    assert_eq!(h.sel(), vec![p.a]);
    h.click(720.0, 320.0, ctrl());
    assert_eq!(h.sel(), vec![p.c]);
}

#[test]
fn shift_click_toggles() {
    let mut h = H::new();
    let p = page(&mut h);
    h.click(320.0, 270.0, none());
    h.click(130.0, 130.0, shift());
    assert_eq!(h.sel(), vec![p.r, p.g]);
    h.click(320.0, 270.0, shift());
    assert_eq!(h.sel(), vec![p.g], "Shift+клик по выделенному убирает его");
    // Клик по одному из нескольких выделенных оставляет только его.
    h.click(620.0, 120.0, shift());
    assert_eq!(h.sel(), vec![p.g, p.s]);
    h.click(620.0, 120.0, none());
    assert_eq!(h.sel(), vec![p.s]);
}

#[test]
fn locked_and_hidden_nodes_are_click_through() {
    let mut h = H::new();
    let p = page(&mut h);
    let top = h.rect(None, 590.0, 90.0, 70.0, 70.0);
    h.set(
        top,
        PropsPatch {
            locked: Some(true),
            ..Default::default()
        },
    );
    h.click(620.0, 120.0, none());
    assert_eq!(h.sel(), vec![p.s], "клик прошёл сквозь заблокированный");
    h.set(
        top,
        PropsPatch {
            locked: Some(false),
            visible: Some(false),
            ..Default::default()
        },
    );
    h.hover(620.0, 120.0);
    assert_eq!(h.ed.overlay().hover, Some(p.s));
    h.click(620.0, 120.0, none());
    assert_eq!(h.sel(), vec![p.s], "скрытый не выбирается");
    // Заблокированный предок закрывает всё поддерево.
    h.set(
        p.g,
        PropsPatch {
            locked: Some(true),
            ..Default::default()
        },
    );
    // Ctrl+клик ищет самый глубокий выбираемый — это уже сам фрейм под группой.
    h.click(130.0, 130.0, ctrl());
    assert_eq!(h.sel(), vec![p.f]);
}

#[test]
fn hover_follows_selection_rules() {
    let mut h = H::new();
    let p = page(&mut h);
    h.hover(130.0, 130.0);
    assert_eq!(h.ed.overlay().hover, Some(p.g));
    h.hover(720.0, 320.0);
    assert_eq!(h.ed.overlay().hover, Some(p.rg));
    h.hover(450.0, 350.0);
    assert_eq!(h.ed.overlay().hover, None, "пустое место фрейма с детьми");
    h.hover(900.0, 700.0);
    assert_eq!(h.ed.overlay().hover, None);
    let events = h.ed.take_events();
    assert!(events.contains(&EditorEvent::NeedsRedraw));
}

#[test]
fn marquee_selects_in_context() {
    let mut h = H::new();
    let p = page(&mut h);
    // С пустого холста: объекты верхнего уровня; фрейм — только целиком в рамке.
    h.drag((560.0, 60.0), (900.0, 380.0), none());
    assert_eq!(h.sel(), vec![p.s, p.rg]);
    h.drag((50.0, 50.0), (900.0, 380.0), none());
    assert_eq!(h.sel(), vec![p.s, p.rg], "фрейм задет, но не охвачен");
    h.drag((50.0, 50.0), (900.0, 450.0), none());
    assert_eq!(h.sel(), vec![p.f, p.s, p.rg]);
    // Внутри фрейма: его дети, которые задевает рамка. Сначала снять выделение:
    // нажатие внутри общей рамки выделенных тащило бы их.
    h.click(900.0, 700.0, none());
    h.drag((480.0, 380.0), (210.0, 140.0), none());
    assert_eq!(h.sel(), vec![p.g, p.r]);
    // Скрытые и заблокированные не попадают.
    h.set(
        p.r,
        PropsPatch {
            visible: Some(false),
            ..Default::default()
        },
    );
    h.drag((480.0, 380.0), (210.0, 140.0), none());
    assert_eq!(h.sel(), vec![p.g]);
    // Shift добавляет к выделению.
    h.click(620.0, 120.0, none());
    h.drag((480.0, 380.0), (210.0, 140.0), shift());
    assert_eq!(h.sel(), vec![p.s, p.g]);
    // Во время рамки оверлей показывает её, после — нет.
    h.drag_hold((480.0, 380.0), (210.0, 140.0), none());
    assert!(h.ed.overlay().marquee.is_some());
    h.up(210.0, 140.0, none());
    assert!(h.ed.overlay().marquee.is_none());
    let _ = p.b;
}

#[test]
fn escape_during_marquee_restores_selection() {
    let mut h = H::new();
    let p = page(&mut h);
    h.click(620.0, 120.0, none());
    h.drag_hold((480.0, 380.0), (210.0, 140.0), shift());
    assert_eq!(h.sel(), vec![p.s, p.g, p.r]);
    h.key("Escape", none());
    assert_eq!(h.sel(), vec![p.s]);
    assert!(!h.ed.is_busy());
}

#[test]
fn deepest_click_uses_cmd_on_mac() {
    let mut h = H::with_platform(Platform::Mac);
    let p = page(&mut h);
    h.click(130.0, 130.0, Modifiers::META);
    assert_eq!(h.sel(), vec![p.a]);
    // Ctrl на macOS — не главная клавиша: обычный клик, и он выбирает соседа a
    // в том же контексте, а не самый глубокий узел под группой.
    h.click(200.0, 140.0, ctrl());
    assert_eq!(h.sel(), vec![p.b]);
    h.click(450.0, 350.0, none());
    h.click(130.0, 130.0, ctrl());
    assert_eq!(h.sel(), vec![p.g], "без контекста — группа целиком");
}

/// Наведение берёт рамку и родителей выделения из кэша по версии выделения и
/// счётчику сцены: ручки следуют за выделением из панели, за правкой документа,
/// за отменой и за сменой страницы; повторный выбор того же списка не шлёт
/// «выделение изменилось», а повторы в списке убираются.
#[test]
fn hover_follows_selection_and_document_changes() {
    let mut h = H::new();
    let a = h.rect(None, 100.0, 100.0, 100.0, 100.0);
    let b = h.rect(None, 400.0, 100.0, 100.0, 100.0);
    let resize = |h: &mut H, x: f64, y: f64| {
        h.hover(x, y);
        matches!(h.ed.cursor(), Cursor::Resize { .. })
    };
    h.ed.select(&[a, a, b, a]);
    assert_eq!(h.sel(), vec![a, b], "повторы убраны, порядок входа");
    h.ed.select(&[a]);
    h.ed.take_events();
    assert!(resize(&mut h, 200.0, 200.0), "угол a");
    h.ed.select(&[a]);
    assert!(
        !h.ed
            .take_events()
            .iter()
            .any(|e| matches!(e, EditorEvent::SelectionChanged(_))),
        "тот же список — не изменение"
    );
    // Выделение из панели: ручки у b, у a — нет.
    h.ed.select(&[b]);
    assert!(h
        .ed
        .take_events()
        .contains(&EditorEvent::SelectionChanged(vec![b])));
    assert!(!resize(&mut h, 200.0, 200.0));
    assert!(resize(&mut h, 500.0, 200.0), "угол b");
    // Правка документа двигает b — ручки за ним.
    h.set(b, patch(600.0, 100.0, 100.0, 100.0));
    assert!(!resize(&mut h, 500.0, 200.0));
    assert!(resize(&mut h, 700.0, 200.0));
    // Отмена правки возвращает b и ручки.
    h.ed.execute(CommandId::Undo);
    assert!(resize(&mut h, 500.0, 200.0));
    // Другая страница: выделение снято, ручек нет, наведение не берёт старую рамку.
    let p2 = h.ed.add_page("Вторая").unwrap();
    h.ed.set_page(p2).unwrap();
    assert!(!resize(&mut h, 500.0, 200.0));
    let c = h.rect(None, 400.0, 100.0, 100.0, 100.0);
    h.ed.select(&[c]);
    assert!(resize(&mut h, 500.0, 200.0), "угол c на второй странице");
}
