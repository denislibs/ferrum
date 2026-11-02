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
fn long_names_are_searched_by_their_start_and_returned_whole() {
    let mut h = H::new();
    let inside = format!("{}я", "а".repeat(FIND_NAME_CHARS - 1));
    let past = format!("{}ю", "а".repeat(FIND_NAME_CHARS));
    named(&mut h, None, &inside, 0.0, 0.0);
    named(&mut h, None, &past, 100.0, 0.0);
    // Буква в пределах индекса находится, за пределом — нет (решение о длине имени).
    assert_eq!(names(&h, "я", 5), [inside]);
    assert!(names(&h, "ю", 5).is_empty());
    assert_eq!(fuzzy_score(&past, "ю"), None);
    // В строке результата имя не обрезается индексом — только `MAX_NAME_CHARS`.
    let very_long = "б".repeat(MAX_NAME_CHARS * 4);
    named(&mut h, None, &very_long, 200.0, 0.0);
    let found = h.ed.find_layers("б", 5);
    assert_eq!(found[0].name.chars().count(), MAX_NAME_CHARS);
    assert_eq!(found[0].kind, NodeKind::Rect);
}

#[test]
fn query_longer_than_the_indexed_name_finds_nothing_quickly() {
    let mut h = H::new();
    named(&mut h, None, &"я".repeat(256), 0.0, 0.0);
    assert_eq!(names(&h, &"я".repeat(32), 5).len(), 1);
    assert!(names(&h, &"я".repeat(128), 5).is_empty());
}

#[test]
fn undo_refreshes_the_index() {
    let mut h = H::new();
    let id = named(&mut h, None, "Старое", 0.0, 0.0);
    assert_eq!(names(&h, "старое", 5), ["Старое"]);
    h.ed.rename_node(id, "Новое");
    assert_eq!(names(&h, "новое", 5), ["Новое"]);
    assert!(names(&h, "старое", 5).is_empty());
    h.ed.execute(CommandId::Undo);
    assert_eq!(names(&h, "старое", 5), ["Старое"]);
    assert!(names(&h, "новое", 5).is_empty());
}

#[test]
fn yo_in_the_other_layout_is_found() {
    let mut h = H::new();
    named(&mut h, None, "Ёлка", 0.0, 0.0);
    // «`» на месте «ё» в ЙЦУКЕН: «`krf» — «ёлка».
    assert_eq!(names(&h, "`krf", 5), ["Ёлка"]);
    assert_eq!(names(&h, "елка", 5), ["Ёлка"]);
}

/// Счёт по определению (решение 1 плана), без оптимизаций индекса: с ним сверяются
/// и быстрый счёт, и отсечение кандидатов.
mod reference {
    fn fold(s: &str) -> Vec<char> {
        s.chars()
            .flat_map(char::to_lowercase)
            .map(|c| if c == 'ё' { 'е' } else { c })
            .collect()
    }

    fn is_start(t: &[char], j: usize) -> bool {
        j == 0 || !t[j - 1].is_alphanumeric()
    }

    fn score(t: &[char], pos: &[usize]) -> i32 {
        let mut s = 0;
        for (i, &j) in pos.iter().enumerate() {
            s += 1;
            if is_start(t, j) {
                s += 8;
            }
            if i > 0 {
                let gap = j - pos[i - 1] - 1;
                s += if gap == 0 { 5 } else { -(gap.min(3) as i32) };
            }
        }
        s += if pos[0] == 0 {
            6
        } else {
            -(pos[0].min(3) as i32)
        };
        if pos.len() == t.iter().filter(|c| !c.is_whitespace()).count() {
            s += 10;
        }
        s
    }

    fn align(t: &[char], q: &[char], prefer_start: bool) -> Option<Vec<usize>> {
        let mut pos = Vec::new();
        let mut from = 0;
        for &qc in q {
            let start = (from..t.len()).find(|&j| prefer_start && t[j] == qc && is_start(t, j));
            let j = start.or_else(|| (from..t.len()).find(|&j| t[j] == qc))?;
            pos.push(j);
            from = j + 1;
        }
        Some(pos)
    }

    fn substring(t: &[char], q: &[char]) -> Option<Vec<usize>> {
        let at: Vec<usize> = (0..(t.len() + 1).saturating_sub(q.len()))
            .filter(|&i| t[i..i + q.len()] == *q)
            .collect();
        let i = at
            .iter()
            .find(|&&i| is_start(t, i))
            .or(at.first())
            .copied()?;
        Some((i..i + q.len()).collect())
    }

    /// Один вариант запроса: без пробелов, в нижнем регистре, та же раскладка.
    pub fn score_plain(text: &str, query: &str) -> Option<i32> {
        let t = fold(text);
        let q: Vec<char> = fold(query)
            .into_iter()
            .filter(|c| !c.is_whitespace())
            .collect();
        if q.is_empty() {
            return None;
        }
        let greedy = align(&t, &q, false)?;
        let mut best = score(&t, &greedy);
        for pos in [align(&t, &q, true), substring(&t, &q)]
            .into_iter()
            .flatten()
        {
            best = best.max(score(&t, &pos));
        }
        Some(best)
    }
}

/// Простой генератор: тесту нужны повторяемые строки, зависимости не нужны.
struct Rng(u64);

impl Rng {
    fn next(&mut self, n: usize) -> usize {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 % n as u64) as usize
    }

    fn text(&mut self, alphabet: &[char], max: usize) -> String {
        let len = self.next(max + 1);
        (0..len)
            .map(|_| alphabet[self.next(alphabet.len())])
            .collect()
    }
}

// Буквы только кириллицы и знаки вне таблицы раскладок: у запроса один вариант.
const ALPHABET: [char; 7] = ['а', 'б', 'в', 'А', ' ', '-', '1'];

#[test]
fn fast_scoring_matches_the_definition() {
    let mut rng = Rng(0x9E37_79B9_7F4A_7C15);
    let mut matched = 0;
    for _ in 0..20_000 {
        let text = rng.text(&ALPHABET, 24);
        let query = rng.text(&ALPHABET, 6);
        let want = reference::score_plain(&text, &query);
        matched += usize::from(want.is_some());
        assert_eq!(fuzzy_score(&text, &query), want, "{text:?} / {query:?}");
    }
    assert!(matched > 3000, "сравнение не вхолостую: {matched}");
}

#[test]
fn top_results_with_pruning_match_the_full_sort() {
    let mut rng = Rng(0x2545_F491_4F6C_DD1D);
    let mut h = H::new();
    let mut layers = Vec::new();
    for i in 0..400 {
        let name = format!("а{}", rng.text(&ALPHABET, 16));
        // Вид сверху вниз: новый узел — выше, поэтому порядок панели — от последнего.
        layers.push(name.clone());
        named(&mut h, None, &name, i as f64, 0.0);
    }
    layers.reverse();
    for query in ["а", "аб", "а б", "ава", "а-1", "б1", "ввв"] {
        let mut scored: Vec<(i32, usize)> = layers
            .iter()
            .enumerate()
            .filter_map(|(i, n)| reference::score_plain(n, query).map(|s| (s, i)))
            .collect();
        scored.sort_by_key(|&(s, i)| (std::cmp::Reverse(s), i));
        for limit in [1, 5, 50, 200] {
            let want: Vec<String> = scored
                .iter()
                .take(limit)
                .map(|&(_, i)| layers[i].clone())
                .collect();
            assert_eq!(names(&h, query, limit), want, "{query:?}, {limit}");
        }
    }
}

/// Те же значения — в `apps/web/unit/fuzzy.test.ts`: счёт ядра и интерфейса один.
#[test]
fn score_vectors_shared_with_the_interface() {
    for (text, query, score) in [
        ("Кнопка отправить", "кно", 27),
        ("Кнопка отправить", "котп", 33),
        ("Order summary", "su", 12),
        ("Ordersum", "su", 4),
        ("Фон", "фон", 37),
        ("Слой 5", "слой 5", 51),
        ("Слой 59", "слой 5", 41),
        ("Вставить поверх", "вп", 21),
        ("Шапка сайта", "шап сай", 45),
        ("Order", "щквук", 49),
    ] {
        assert_eq!(fuzzy_score(text, query), Some(score), "{text} / {query}");
    }
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

#[test]
fn hidden_layer_is_selected_without_moving_the_view() {
    let mut h = H::new();
    let hidden = named(&mut h, None, "Скрытый", 5000.0, 3000.0);
    h.set(
        hidden,
        PropsPatch {
            visible: Some(false),
            ..Default::default()
        },
    );
    let frame = h.frame(None, 9000.0, 9000.0, 100.0, 100.0);
    let child = named(&mut h, Some(frame), "Ребёнок", 6.0, 6.0);
    h.set(
        frame,
        PropsPatch {
            visible: Some(false),
            ..Default::default()
        },
    );
    let before = h.ed.camera().clone();
    for id in [hidden, child] {
        assert!(h.ed.reveal_layer(id));
        assert_eq!(h.sel(), [id]);
        assert_eq!(h.ed.camera().center(), before.center());
        assert_eq!(h.ed.camera().zoom(), before.zoom());
    }
}

#[test]
fn name_preview_refreshes_the_index() {
    let mut doc = Document::new();
    let page = doc.pages()[0].0;
    let id = doc
        .transact(page, vec![], |p| {
            let id = p.create_node(None, None, NodeKind::Rect)?;
            let name = PropsPatch {
                name: Some("Старое".into()),
                ..Default::default()
            };
            p.set_props(id, &name)?;
            Ok(vec![id])
        })
        .unwrap()[0];
    let mut scene = ferrum_scene::Scene::from_page(doc.page(page).unwrap());
    let mut index = LayerIndex::default();
    assert_eq!(index.find(&scene, "старое", 5).len(), 1);
    let patch = PropsPatch {
        name: Some("Новое".into()),
        ..Default::default()
    };
    scene.set_preview(&[(id, patch)]);
    assert_eq!(index.find(&scene, "новое", 5).len(), 1);
    assert!(index.find(&scene, "старое", 5).is_empty());
    scene.set_preview(&[]);
    assert_eq!(index.find(&scene, "старое", 5).len(), 1);
    assert!(index.find(&scene, "новое", 5).is_empty());
}
