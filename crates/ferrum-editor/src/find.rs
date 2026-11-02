//! Поиск слоёв для «Быстрых действий» (спека подсистемы 2, раздел 7) и переход
//! к слою.
//!
//! Поиск нечёткий: буквы запроса должны встретиться в имени по порядку
//! (подпоследовательность), а вес растёт за начало слова, буквы подряд и начало
//! имени и падает за пропуски. Регистр и «ё» не важны, раскладка тоже: запрос
//! пробуется как набран и переведённым ЙЦУКЕН ↔ QWERTY («щквук» находит
//! «order»). Пробелы запроса не учитываются. Тот же счёт — у поиска команд в
//! интерфейсе (`apps/web/src/ui/fuzzy.ts`).
//!
//! Обход — вся текущая страница сверху вниз, как строки панели слоёв; имя
//! участвует первыми `FIND_NAME_CHARS` символами. Результат — лучшие `limit`
//! по весу, при равном весе — в порядке панели.

use ferrum_doc::{NodeId, NodeKind};
use ferrum_scene::{Scene, Slot};

use crate::actions::FIT_PADDING_PX;
use crate::editor::Editor;
use crate::geometry::world_box;

/// Наибольшая длина запроса поиска слоёв в байтах UTF-8 (`FindLayers.query`).
pub const MAX_FIND_QUERY_BYTES: usize = 256;
/// Наибольшее число найденных слоёв за запрос (`FindLayers.limit`).
pub const MAX_FIND_LIMIT: usize = 200;
/// Сколько первых символов имени участвует в поиске и уходит в результат: имя из
/// файла может быть мегабайтным.
pub const FIND_NAME_CHARS: usize = 256;

/// Найденный слой.
#[derive(Debug, Clone, PartialEq)]
pub struct FoundLayer {
    pub id: NodeId,
    /// Имя — первые `FIND_NAME_CHARS` символов.
    pub name: String,
    pub kind: NodeKind,
}

const EN: &str = "qwertyuiop[]asdfghjkl;'zxcvbnm,.`";
const RU: &str = "йцукенгшщзхъфывапролджэячсмитьбюё";

/// Символ для сравнения: нижний регистр, «ё» как «е». Латиница и кириллица —
/// без таблиц Unicode (`to_lowercase` на 100 000 имён — основная цена поиска).
fn fold(c: char, out: &mut Vec<char>) {
    match c {
        'a'..='z' | 'а'..='я' | '0'..='9' | ' ' => out.push(c),
        'A'..='Z' => out.push(c.to_ascii_lowercase()),
        'А'..='Я' => out.push(char::from_u32(c as u32 + 32).unwrap_or(c)),
        'ё' | 'Ё' => out.push('е'),
        _ if c.is_ascii() => out.push(c),
        _ => {
            for l in c.to_lowercase() {
                out.push(if l == 'ё' { 'е' } else { l });
            }
        }
    }
}

fn folded(s: &str, max_chars: usize, out: &mut Vec<char>) {
    out.clear();
    for c in s.chars().take(max_chars) {
        fold(c, out);
    }
}

/// Запрос в обеих раскладках: как набран, ЙЦУКЕН → QWERTY и обратно; без
/// пробелов, в нижнем регистре, «ё» как «е», без повторов. Пустой запрос — пусто.
pub fn layout_variants(query: &str) -> Vec<Vec<char>> {
    let mut base = Vec::new();
    for c in query.chars().filter(|c| !c.is_whitespace()) {
        for l in c.to_lowercase() {
            base.push(l);
        }
    }
    if base.is_empty() {
        return Vec::new();
    }
    let swap = |from: &str, to: &str| -> Vec<char> {
        let to: Vec<char> = to.chars().collect();
        base.iter()
            .map(|c| from.chars().position(|f| f == *c).map_or(*c, |i| to[i]))
            .collect()
    };
    let mut out: Vec<Vec<char>> = Vec::new();
    for v in [base.clone(), swap(EN, RU), swap(RU, EN)] {
        let v: Vec<char> = v
            .into_iter()
            .map(|c| if c == 'ё' { 'е' } else { c })
            .collect();
        if !out.contains(&v) {
            out.push(v);
        }
    }
    out
}

/// Начало слова: первый символ или после символа, который не буква и не цифра.
fn is_start(t: &[char], j: usize) -> bool {
    if j == 0 {
        return true;
    }
    let p = t[j - 1];
    let alnum = match p {
        'a'..='z' | '0'..='9' | 'а'..='я' => true,
        _ if p.is_ascii() => false,
        _ => p.is_alphanumeric(),
    };
    !alnum
}

/// Совпадение буквы — 1; начало слова — ещё 8; буква сразу за прошлой — ещё 5,
/// иначе минус пропуск (не больше 3); начало имени — ещё 6, иначе минус сдвиг
/// (не больше 3); имя целиком (без пробелов) — ещё 10.
const MATCH: i32 = 1;
const WORD_START: i32 = 8;
const CONSECUTIVE: i32 = 5;
const PREFIX: i32 = 6;
const MAX_GAP: usize = 3;
const EXACT: i32 = 10;

fn score_positions(t: &[char], pos: &[usize]) -> i32 {
    let mut s = 0;
    for (i, &j) in pos.iter().enumerate() {
        s += MATCH;
        if is_start(t, j) {
            s += WORD_START;
        }
        if i > 0 {
            let gap = j - pos[i - 1] - 1;
            if gap == 0 {
                s += CONSECUTIVE;
            } else {
                s -= gap.min(MAX_GAP) as i32;
            }
        }
    }
    match pos.first() {
        Some(0) => s += PREFIX,
        Some(&p) => s -= p.min(MAX_GAP) as i32,
        None => {}
    }
    // Совпали все буквы имени (пробелы запроса не учитываются — и имени тоже).
    if pos.len() == t.iter().filter(|c| !c.is_whitespace()).count() {
        s += EXACT;
    }
    s
}

/// Буквы запроса по порядку: каждая — первое вхождение после прошлой; с
/// `prefer_start` — сначала вхождение в начале слова, если оно есть.
fn align(t: &[char], q: &[char], prefer_start: bool, pos: &mut Vec<usize>) -> bool {
    pos.clear();
    let mut from = 0;
    for &qc in q {
        let start = if prefer_start {
            (from..t.len()).find(|&j| t[j] == qc && is_start(t, j))
        } else {
            None
        };
        let Some(j) = start.or_else(|| (from..t.len()).find(|&j| t[j] == qc)) else {
            return false;
        };
        pos.push(j);
        from = j + 1;
    }
    true
}

/// Запрос подстрокой: лучшее из вхождений (в начале слова — первое такое).
fn substring(t: &[char], q: &[char], pos: &mut Vec<usize>) -> bool {
    if q.len() > t.len() {
        return false;
    }
    let mut best = None;
    for i in 0..=t.len() - q.len() {
        if t[i..i + q.len()] == *q {
            if is_start(t, i) {
                best = Some(i);
                break;
            }
            best.get_or_insert(i);
        }
    }
    let Some(i) = best else { return false };
    pos.clear();
    pos.extend(i..i + q.len());
    true
}

/// Вес совпадения запроса (одного варианта, уже сложенного) с текстом (сложенным);
/// `None` — буквы запроса не встречаются по порядку.
pub fn fuzzy_score_folded(t: &[char], q: &[char], pos: &mut Vec<usize>) -> Option<i32> {
    if q.is_empty() || !align(t, q, false, pos) {
        return None;
    }
    let mut best = score_positions(t, pos);
    if align(t, q, true, pos) {
        best = best.max(score_positions(t, pos));
    }
    if substring(t, q, pos) {
        best = best.max(score_positions(t, pos));
    }
    Some(best)
}

/// Вес совпадения запроса с текстом по всем раскладкам; `None` — не совпало.
pub fn fuzzy_score(text: &str, query: &str) -> Option<i32> {
    let mut t = Vec::new();
    folded(text, FIND_NAME_CHARS, &mut t);
    let mut pos = Vec::new();
    layout_variants(query)
        .iter()
        .filter_map(|q| fuzzy_score_folded(&t, q, &mut pos))
        .max()
}

/// Сложенные имена страницы в порядке панели (сверху вниз) — одним массивом:
/// повторный запрос не трогает узлы сцены и не складывает имена заново. Живёт,
/// пока не сменились экземпляр сцены и её метка состава и имён.
#[derive(Debug, Default)]
pub struct LayerIndex {
    key: Option<(u64, u64)>,
    /// Слот, узел и диапазон его имени в `chars`.
    entries: Vec<(Slot, NodeId, u32, u32)>,
    chars: Vec<char>,
}

impl LayerIndex {
    fn sync(&mut self, scene: &Scene) {
        let key = (scene.generation(), scene.structure_stamp());
        if self.key == Some(key) {
            return;
        }
        self.key = Some(key);
        self.entries.clear();
        self.chars.clear();
        let mut name = Vec::new();
        let mut stack: Vec<Slot> = scene.roots().to_vec();
        while let Some(s) = stack.pop() {
            if scene.is_draft_slot(s) {
                continue;
            }
            let n = scene.at(s);
            folded(&n.props.name, FIND_NAME_CHARS, &mut name);
            let from = self.chars.len() as u32;
            self.chars.extend_from_slice(&name);
            self.entries.push((s, n.id, from, self.chars.len() as u32));
            // Дети — в прямом порядке: верхний (последний) снимается первым.
            stack.extend(n.children.iter().copied());
        }
    }

    /// Слои страницы `scene` по нечёткому запросу: не больше `limit` (и
    /// `MAX_FIND_LIMIT`), лучшие первыми, при равном весе — в порядке панели
    /// слоёв (сверху вниз). Пустой запрос — пусто. Черновики жеста не ищутся.
    pub fn find(&mut self, scene: &Scene, query: &str, limit: usize) -> Vec<FoundLayer> {
        let limit = limit.min(MAX_FIND_LIMIT);
        let variants = layout_variants(query);
        if variants.is_empty() || limit == 0 {
            return Vec::new();
        }
        self.sync(scene);
        // (вес, порядок в панели)
        let mut hits: Vec<(i32, u32)> = Vec::new();
        let mut pos = Vec::new();
        for (i, &(_, _, a, b)) in self.entries.iter().enumerate() {
            let t = &self.chars[a as usize..b as usize];
            let best = variants
                .iter()
                .filter_map(|q| fuzzy_score_folded(t, q, &mut pos))
                .max();
            if let Some(score) = best {
                hits.push((score, i as u32));
            }
        }
        let key = |h: &(i32, u32)| (std::cmp::Reverse(h.0), h.1);
        let mut out = Vec::new();
        // Черновики и узлы, чей слот уже занят другим, пропускаются: отбор — с запасом.
        while out.len() < limit && !hits.is_empty() {
            let want = (limit - out.len()).min(hits.len());
            if hits.len() > want {
                hits.select_nth_unstable_by_key(want - 1, key);
            }
            let mut head: Vec<(i32, u32)> = hits.drain(..want).collect();
            head.sort_unstable_by_key(key);
            for (_, i) in head {
                let (slot, id, _, _) = self.entries[i as usize];
                let Some(n) = scene.node_at(slot).filter(|n| n.id == id) else {
                    continue;
                };
                if scene.is_draft_slot(slot) {
                    continue;
                }
                out.push(FoundLayer {
                    id,
                    name: n.props.name.chars().take(FIND_NAME_CHARS).collect(),
                    kind: n.props.kind,
                });
            }
        }
        out
    }
}

impl Editor {
    /// Слои текущей страницы по нечёткому запросу (`LayerIndex::find`); сложенные
    /// имена страницы держатся между запросами.
    pub fn find_layers(&self, query: &str, limit: usize) -> Vec<FoundLayer> {
        self.find_index.borrow_mut().find(&self.scene, query, limit)
    }

    /// Переход к слою (выбор в «Быстрых действиях»): слой выделяется, как из
    /// панели, и показывается в виде — уже целиком виден: вид прежний; влезает
    /// при нынешнем масштабе: вид сдвигается к его центру; не влезает: вписывается,
    /// как «Показать выделение». У скрытого слоя (рамки нет) — только выделение.
    /// Слоя нет на странице — `false`, ничего не меняется.
    pub fn reveal_layer(&mut self, id: NodeId) -> bool {
        let Some(slot) = self.scene.slot(id) else {
            return false;
        };
        if self.scene.is_draft_slot(slot) {
            return false;
        }
        self.select(&[id]);
        if let Some(b) = world_box(&self.scene, slot) {
            let view = self.camera.visible_world_rect();
            let inside = b.x0 >= view.x0 && b.y0 >= view.y0 && b.x1 <= view.x1 && b.y1 <= view.y1;
            if !inside {
                let v = self.camera.viewport();
                let zoom = self.camera.zoom();
                let fits = b.width() * zoom <= v.width - 2.0 * FIT_PADDING_PX
                    && b.height() * zoom <= v.height - 2.0 * FIT_PADDING_PX;
                if fits {
                    self.camera.set_center(b.center());
                } else {
                    self.camera.fit(b, FIT_PADDING_PX);
                }
            }
        }
        self.emit();
        true
    }
}
