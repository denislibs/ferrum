//! Поиск слоёв и переход к слою для «Быстрых действий» (спека подсистемы 2,
//! раздел 7): нечёткий поиск по подпоследовательности с весом, без учёта
//! регистра, «ё» и раскладки; переход выделяет слой и показывает его в виде.

mod common;

use common::*;
use ferrum_doc::*;
use ferrum_editor::*;

fn named(h: &mut H, parent: Option<NodeId>, name: &str, x: f64, y: f64) -> NodeId {
    let id = h.rect(parent, x, y, 40.0, 40.0);
    h.set(
        id,
        PropsPatch {
            name: Some(name.into()),
            ..Default::default()
        },
    );
    id
}

fn names(h: &H, query: &str, limit: usize) -> Vec<String> {
    h.ed.find_layers(query, limit)
        .into_iter()
        .map(|f| f.name)
        .collect()
}

#[test]
fn subsequence_with_word_starts_first() {
    assert!(fuzzy_score("Кнопка отправить", "кно").is_some());
    assert!(
        fuzzy_score("Кнопка отправить", "котп").is_some(),
        "подпоследовательность"
    );
    assert_eq!(fuzzy_score("Кнопка", "кнопки"), None);
    assert_eq!(fuzzy_score("Кнопка", ""), None);
    assert_eq!(fuzzy_score("Кнопка", "   "), None);
    // Начало слова весит больше, чем буквы в середине.
    let start = fuzzy_score("Order summary", "su").unwrap();
    let middle = fuzzy_score("Ordersum", "su").unwrap();
    assert!(start > middle, "{start} > {middle}");
    // Буквы подряд весят больше, чем вразброс.
    let run = fuzzy_score("Карточка товара", "кар").unwrap();
    let spread = fuzzy_score("Кнопка в ряд", "кар").unwrap();
    assert!(run > spread, "{run} > {spread}");
    // Имя целиком — выше, чем его начало в длинном имени.
    assert!(fuzzy_score("Фон", "фон").unwrap() > fuzzy_score("Фон карточки", "фон").unwrap());
    // И с пробелом: «Слой 5» целиком выше «Слой 59».
    assert!(fuzzy_score("Слой 5", "слой 5").unwrap() > fuzzy_score("Слой 59", "слой 5").unwrap());
}

#[test]
fn case_yo_and_keyboard_layout_do_not_matter() {
    assert_eq!(fuzzy_score("ЁЛКА", "елка"), fuzzy_score("ёлка", "ЕЛКА"));
    assert!(fuzzy_score("Ёлка", "елк").is_some());
    // «щквук» набрано в русской раскладке вместо «order».
    assert_eq!(fuzzy_score("Order", "щквук"), fuzzy_score("Order", "order"));
    // И наоборот: «afqk» — это «файл».
    assert!(fuzzy_score("Файл", "afqk").is_some());
    // Пробелы запроса не учитываются.
    assert!(fuzzy_score("Шапка сайта", "шап сай").is_some());
}

#[test]
fn finds_on_the_current_page_best_first_then_panel_order() {
    let mut h = H::new();
    let f = h.frame(None, 0.0, 0.0, 400.0, 400.0);
    named(&mut h, Some(f), "Кнопка", 10.0, 10.0);
    named(&mut h, Some(f), "Иконка", 60.0, 10.0);
    named(&mut h, None, "Кнопка отправки", 500.0, 0.0);
    let bg = named(&mut h, None, "Фон", 600.0, 0.0);
    // «кнопка» целиком — первой; остальное с «кн…» — по порядку панели.
    assert_eq!(names(&h, "кнопка", 50), ["Кнопка", "Кнопка отправки"]);
    // «ка» с начала слова весит больше, чем в середине («Иконка» — последней).
    assert_eq!(names(&h, "ка", 50), ["Кнопка отправки", "Кнопка", "Иконка"]);
    // Равный вес — порядок панели: сверху вниз (верхний слой первым).
    assert_eq!(names(&h, "нопк", 50), ["Кнопка отправки", "Кнопка"]);
    // Раскладка: «ryjgrf» — «кнопка».
    assert_eq!(names(&h, "ryjgrf", 50)[0], "Кнопка");
    assert!(names(&h, "", 50).is_empty());
    assert!(names(&h, "zzz", 50).is_empty());
    // Имена сложены заранее, но переименование и удаление видны сразу.
    assert_eq!(names(&h, "фон", 50), ["Фон"]);
    h.ed.rename_node(bg, "Задник");
    assert!(names(&h, "фон", 50).is_empty());
    assert_eq!(names(&h, "задн", 50), ["Задник"]);
    h.ed.select(&[bg]);
    h.ed.execute(CommandId::Delete);
    assert!(names(&h, "задн", 50).is_empty());
    // Предел — сколько просили, но не больше MAX_FIND_LIMIT.
    assert_eq!(names(&h, "к", 2).len(), 2);
    for i in 0..(MAX_FIND_LIMIT + 10) {
        named(&mut h, None, &format!("Слой {i}"), 0.0, 500.0);
    }
    assert_eq!(h.ed.find_layers("слой", 10_000).len(), MAX_FIND_LIMIT);
    // Другая страница — свои слои.
    h.ed.create_page().unwrap();
    assert!(names(&h, "кнопка", 50).is_empty());
}

#[test]
fn long_names_are_clipped() {
    let mut h = H::new();
    let long = "я".repeat(FIND_NAME_CHARS * 4);
    named(&mut h, None, &long, 0.0, 0.0);
    let found = h.ed.find_layers("я", 5);
    assert_eq!(found[0].name.chars().count(), FIND_NAME_CHARS);
    assert_eq!(found[0].kind, NodeKind::Rect);
}

#[test]
fn reveal_selects_and_shows_the_layer() {
    let mut h = H::new();
    let f = h.frame(None, 0.0, 0.0, 400.0, 400.0);
    let near = named(&mut h, Some(f), "Рядом", 100.0, 100.0);
    let far = named(&mut h, None, "Далеко", 5000.0, 3000.0);
    let huge = h.add(
        None,
        NodeKind::Rect,
        patch(-20_000.0, 0.0, 10_000.0, 8000.0),
    );
    let before = h.ed.camera().clone();
    let steps = h.ed.step_count();

    // Виден целиком — вид прежний, слой выделен.
    assert!(h.ed.reveal_layer(near));
    assert_eq!(h.sel(), [near]);
    assert_eq!(h.ed.camera().center(), before.center());
    assert_eq!(h.ed.camera().zoom(), before.zoom());

    // Не виден, но влезает: масштаб прежний, центр — центр слоя.
    assert!(h.ed.reveal_layer(far));
    assert_eq!(h.sel(), [far]);
    assert_eq!(h.ed.camera().zoom(), before.zoom());
    let c = h.ed.camera().center();
    assert!(close(c.x, 5020.0) && close(c.y, 3020.0), "{c:?}");

    // Не влезает — вписан с отступом, как «Показать выделение».
    assert!(h.ed.reveal_layer(huge));
    assert!(h.ed.camera().zoom() < before.zoom());
    let view = h.ed.camera().visible_world_rect();
    assert!(view.x0 <= -20_000.0 && view.x1 >= -10_000.0, "{view:?}");

    // Выбор — не шаг истории.
    assert_eq!(h.ed.step_count(), steps);
    // Слоя нет на странице (удалён, на другой странице) — ничего не меняется.
    h.ed.select(&[near]);
    h.ed.execute(CommandId::Delete);
    let cam = h.ed.camera().clone();
    assert!(!h.ed.reveal_layer(near));
    assert!(h.sel().is_empty());
    assert_eq!(h.ed.camera().center(), cam.center());
    h.ed.create_page().unwrap();
    assert!(!h.ed.reveal_layer(far));
}
