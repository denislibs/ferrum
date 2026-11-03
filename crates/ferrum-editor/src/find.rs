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
//!
//! Цель — 30 мс на 100 000 узлов (первый запрос после правки на длинных именах —
//! 40 мс), поэтому работа на имя ограничена: маска букв
//! отсекает имя без проверки строки, верхняя граница веса — имя, которое уже не
//! войдёт в лучшие `limit`, а выравнивания линейны по длине имени (начала слов
//! посчитаны заранее, подстрока — КМП). Веса от этого не меняются.

use std::cmp::Reverse;
use std::collections::BinaryHeap;
use std::sync::LazyLock;

use ferrum_doc::{NodeId, NodeKind};
use ferrum_scene::{Scene, Slot};

use crate::actions::FIT_PADDING_PX;
use crate::editor::Editor;
use crate::geometry::world_box;
use crate::pages::MAX_NAME_CHARS;

/// Наибольшая длина запроса поиска слоёв в байтах UTF-8 (`FindLayers.query`).
pub const MAX_FIND_QUERY_BYTES: usize = 256;
/// Наибольшее число найденных слоёв за запрос (`FindLayers.limit`).
pub const MAX_FIND_LIMIT: usize = 200;
/// Сколько первых символов имени участвует в поиске (то же — `MAX_TEXT_CHARS` в
/// `fuzzy.ts`): держит бюджет и память индекса; **(поведение)** совпадение дальше
/// этого символа не находится.
pub const FIND_NAME_CHARS: usize = 40;

/// Найденный слой.
#[derive(Debug, Clone, PartialEq)]
pub struct FoundLayer {
    pub id: NodeId,
    /// Имя — первые `MAX_NAME_CHARS` символов: имя из файла может быть мегабайтным.
    pub name: String,
    pub kind: NodeKind,
}

const EN: &str = "qwertyuiop[]asdfghjkl;'zxcvbnm,.`";
const RU: &str = "йцукенгшщзхъфывапролджэячсмитьбюё";

/// Символ для сравнения: нижний регистр, «ё» как «е». Обычные буквы складывает
/// `index_name` без таблиц Unicode, сюда доходит остальное.
fn fold(c: char, out: &mut Vec<char>) {
    if c.is_ascii() {
        out.push(c.to_ascii_lowercase());
        return;
    }
    for l in c.to_lowercase() {
        out.push(if l == 'ё' { 'е' } else { l });
    }
}

/// Запрос в обеих раскладках: как набран, ЙЦУКЕН → QWERTY и обратно; без
/// пробелов, в нижнем регистре, «ё» как «е», без повторов. Пустой запрос — пусто.
/// От запроса берутся первые `FIND_NAME_CHARS` символов вместе с пробелами, как и от
/// имени: вставленное целиком длинное имя находит само себя.
pub fn layout_variants(query: &str) -> Vec<Vec<char>> {
    let base: Vec<char> = query
        .trim()
        .chars()
        .take(FIND_NAME_CHARS)
        .flat_map(char::to_lowercase)
        .take(FIND_NAME_CHARS)
        .filter(|c| !c.is_whitespace())
        .collect();
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

/// Буква или цифра: после такого символа слово не начинается.
fn is_alnum(c: char) -> bool {
    match c {
        'a'..='z' | '0'..='9' | 'а'..='я' => true,
        _ if c.is_ascii() => false,
        _ => c.is_alphanumeric(),
    }
}

/// Бит символа в маске букв имени. Строчные кириллица и латиница не пересекаются:
/// запрос в другой раскладке отсекается без просмотра имени.
fn letter_bit(c: char) -> u64 {
    let c = c as u32;
    1 << (c.wrapping_add((c >> 8).wrapping_mul(4)) & 63)
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

/// Что посчитано об имени заранее. `FIND_NAME_CHARS` входит в биты и в `u8`.
#[derive(Debug, Clone, Copy, Default)]
struct NameFacts {
    /// Начала слов — символы после не буквы и не цифры, бит `j` — символ `j`.
    starts: u64,
    /// Маска букв (`letter_bit`).
    mask: u64,
    len: u8,
    /// Символов без пробелов.
    solid: u8,
    /// Самый длинный участок без пробелов: подстрока длиннее в имени не встретится.
    run: u8,
}

const _: () = assert!(FIND_NAME_CHARS <= 64);

/// Сложенное имя и посчитанное о нём.
#[derive(Clone, Copy)]
struct Name<'a> {
    t: &'a [char],
    f: NameFacts,
}

/// Запись таблицы свёртки: сложенный символ (младшие биты) и что о нём известно.
const FOLDED_BITS: u16 = 0x7FF;
const ALNUM: u16 = 1 << 11;
const SPACE: u16 = 1 << 12;
/// Символ не сворачивается в один из таблицы («İ», «Ⱥ»): считать как обычно.
const NOT_IN_TABLE: u16 = 1 << 13;

/// Свёртка символов до U+07FF (латиница, греческий, кириллица, армянский, иврит,
/// арабский). `to_lowercase` и `is_alphanumeric` — поиск по таблицам Unicode, десятки
/// наносекунд на букву вне ASCII: на 40 символах имени это дольше всего остального.
/// Таблица заполняется ими же, поэтому отличаться от них не может.
static FOLD: LazyLock<[u16; FOLDED_BITS as usize + 1]> = LazyLock::new(|| {
    std::array::from_fn(|i| {
        let c = char::from_u32(i as u32).unwrap_or('\0');
        let mut out = Vec::new();
        fold(c, &mut out);
        match out[..] {
            [l] if l as u32 <= FOLDED_BITS as u32 => {
                let alnum = if is_alnum(l) { ALNUM } else { 0 };
                let space = if l.is_whitespace() { SPACE } else { 0 };
                l as u16 | alnum | space
            }
            _ => NOT_IN_TABLE,
        }
    })
});

/// Буквы без регистра сплошными блоками (китайский, японский, корейский): символ
/// остаётся собой, поиск по таблицам не нужен. Границы сверяет тест ниже.
fn is_caseless_letter(c: char) -> bool {
    matches!(
        c,
        '\u{3041}'..='\u{3096}'
            | '\u{30A1}'..='\u{30FA}'
            | '\u{3400}'..='\u{4DBF}'
            | '\u{4E00}'..='\u{9FFF}'
            | '\u{AC00}'..='\u{D7A3}'
    )
}

/// Складывает первые `FIND_NAME_CHARS` символов имени в конец `chars`. Один проход
/// по локальным переменным: сборка индекса — львиная доля первого запроса после
/// правки имён.
fn index_name(name: &str, chars: &mut Vec<char>) -> NameFacts {
    let mut buf = ['\0'; FIND_NAME_CHARS];
    let (mut n, mut starts, mut mask, mut solid) = (0, 0u64, 0u64, 0u8);
    let (mut run, mut longest, mut after_alnum) = (0u8, 0u8, false);
    let fold_table = &*FOLD;
    let mut slow = Vec::new();
    macro_rules! note {
        ($l:expr, $alnum:expr, $space:expr) => {{
            let (l, space): (char, bool) = ($l, $space);
            starts |= u64::from(!after_alnum) << n;
            buf[n] = l;
            n += 1;
            after_alnum = $alnum;
            mask |= letter_bit(l);
            solid += u8::from(!space);
            run = if space { 0 } else { run + 1 };
            longest = longest.max(run);
        }};
    }
    for c in name.chars().take(FIND_NAME_CHARS) {
        // Символ свёртки бывает длиннее исходного («İ»): лишнее — за пределом.
        if n == FIND_NAME_CHARS {
            break;
        }
        let entry = fold_table.get(c as usize).copied().unwrap_or(NOT_IN_TABLE);
        if entry & NOT_IN_TABLE == 0 {
            let l = char::from_u32((entry & FOLDED_BITS) as u32).unwrap_or(c);
            note!(l, entry & ALNUM != 0, entry & SPACE != 0);
        } else if is_caseless_letter(c) {
            note!(c, true, false);
        } else {
            slow.clear();
            fold(c, &mut slow);
            for &l in slow.iter().take(FIND_NAME_CHARS - n) {
                note!(l, is_alnum(l), l.is_whitespace());
            }
        }
    }
    chars.extend_from_slice(&buf[..n]);
    NameFacts {
        starts,
        mask,
        len: n as u8,
        solid,
        run: longest,
    }
}

fn score_positions(name: Name, pos: &[usize]) -> i32 {
    let mut s = 0;
    for (i, &j) in pos.iter().enumerate() {
        s += MATCH;
        if name.f.starts >> j & 1 == 1 {
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
    // Совпали все буквы имени (пробелы запроса не учитываются — и имени тоже). Имя
    // здесь — его первые `FIND_NAME_CHARS` символов: запрос, равный им, тоже считается
    // именем целиком, даже если само имя длиннее.
    if pos.len() == name.f.solid as usize {
        s += EXACT;
    }
    s
}

/// Буквы запроса по порядку: каждая — первое вхождение после прошлой. Заодно это
/// проверка, что запрос — подпоследовательность имени. Один проход по имени.
fn align_first(t: &[char], q: &[char], pos: &mut Vec<usize>) -> bool {
    pos.clear();
    let mut from = 0;
    for &qc in q {
        let Some(k) = t[from..].iter().position(|&c| c == qc) else {
            return false;
        };
        pos.push(from + k);
        from += k + 1;
    }
    true
}

/// То же, но сначала — вхождение в начале слова, если оно есть. Буква, с которой
/// не начинается ни одно слово, начал слов не просматривает; откат к обычному
/// вхождению идёт по имени один раз.
fn align_starts(name: Name, q: &[char], pos: &mut Vec<usize>) -> bool {
    let mut first_letters = 0;
    let mut bits = name.f.starts;
    while bits != 0 {
        first_letters |= letter_bit(name.t[bits.trailing_zeros() as usize]);
        bits &= bits - 1;
    }
    pos.clear();
    let mut from = 0;
    for &qc in q {
        let mut start = None;
        if first_letters & letter_bit(qc) != 0 {
            let mut bits = name.f.starts.checked_shr(from as u32).unwrap_or(0) << from;
            while bits != 0 {
                let j = bits.trailing_zeros() as usize;
                if name.t[j] == qc {
                    start = Some(j);
                    break;
                }
                bits &= bits - 1;
            }
        }
        let plain = || {
            name.t[from..]
                .iter()
                .position(|&c| c == qc)
                .map(|k| from + k)
        };
        let Some(j) = start.or_else(plain) else {
            return false;
        };
        pos.push(j);
        from = j + 1;
    }
    true
}

/// Запрос, готовый к сравнению с именами.
struct Needle {
    q: Vec<char>,
    /// Маска букв запроса: все должны быть и в имени.
    mask: u64,
    /// Таблица КМП: сколько букв совпавшего начала остаётся после несовпадения.
    fail: Vec<u32>,
    /// Вес букв подстроки без учёта её места в имени: начала слов внутри неё
    /// определяются самим запросом.
    run_score: i32,
}

impl Needle {
    fn new(q: Vec<char>) -> Self {
        let mask = q.iter().fold(0, |m, &c| m | letter_bit(c));
        let mut fail = vec![0u32; q.len()];
        let mut k = 0;
        for i in 1..q.len() {
            while k > 0 && q[i] != q[k] {
                k = fail[k - 1] as usize;
            }
            if q[i] == q[k] {
                k += 1;
            }
            fail[i] = k as u32;
        }
        let inner_starts = q.windows(2).filter(|w| !is_alnum(w[0])).count() as i32;
        let run_score =
            q.len() as i32 * MATCH + (q.len() as i32 - 1) * CONSECUTIVE + inner_starts * WORD_START;
        Self {
            q,
            mask,
            fail,
            run_score,
        }
    }

    /// Больше этого имя набрать не может: все буквы подряд, в началах слов сколько
    /// их есть, начало имени — если первая буква с него и начинается.
    fn bound(&self, name: Name) -> i32 {
        let m = self.q.len();
        let mut b = m as i32 * MATCH
            + (m as i32 - 1) * CONSECUTIVE
            + m.min(name.f.starts.count_ones() as usize) as i32 * WORD_START;
        b += if name.t[0] == self.q[0] { PREFIX } else { -1 };
        if m == name.f.solid as usize {
            b += EXACT;
        }
        b
    }

    /// Вес подстроки на месте `at`; не считая позиций: всё известно из запроса.
    fn substring_score(&self, name: Name, at: usize) -> i32 {
        let mut s = self.run_score;
        if name.f.starts >> at & 1 == 1 {
            s += WORD_START;
        }
        s += if at == 0 {
            PREFIX
        } else {
            -(at.min(MAX_GAP) as i32)
        };
        if self.q.len() == name.f.solid as usize {
            s += EXACT;
        }
        s
    }

    /// Запрос подстрокой, начиная с `from`: вхождение в начале слова, иначе первое.
    /// Если вес `best` не превзойти вхождением не в начале слова, смотрим только
    /// начала: буквы без знаков препинания останавливают сравнение на границе слова,
    /// так что это один проход. Иначе — КМП.
    fn substring(&self, name: Name, from: usize, best: i32) -> Option<usize> {
        let (q, t) = (&self.q[..], name.t);
        if best >= self.run_score - 1 {
            let mut bits = name.f.starts.checked_shr(from as u32).unwrap_or(0) << from;
            while bits != 0 {
                let j = bits.trailing_zeros() as usize;
                if t[j..].starts_with(q) {
                    return Some(j);
                }
                bits &= bits - 1;
            }
            return None;
        }
        let (mut first, mut k) = (None, 0);
        for (i, &c) in t.iter().enumerate().skip(from) {
            while k > 0 && q[k] != c {
                k = self.fail[k - 1] as usize;
            }
            if q[k] == c {
                k += 1;
            }
            if k == q.len() {
                let at = i + 1 - k;
                if name.f.starts >> at & 1 == 1 {
                    return Some(at);
                }
                first.get_or_insert(at);
                k = self.fail[k - 1] as usize;
            }
        }
        first
    }

    /// Точный вес имени — лучшее из трёх выравниваний; `None` — запрос не встречается
    /// по порядку или имя заведомо не наберёт больше `floor`. Выравнивание не считается,
    /// только когда вес уже равен верхней границе.
    fn score(&self, name: Name, floor: i32, pos: &mut Vec<usize>) -> Option<i32> {
        let q = &self.q[..];
        // В запросе нет пробелов: ему нужно столько же символов без пробелов.
        if q.is_empty() || q.len() > name.f.solid as usize || self.mask & !name.f.mask != 0 {
            return None;
        }
        let bound = self.bound(name);
        if bound <= floor || !align_first(name.t, q, pos) {
            return None;
        }
        // Подстрока не начнётся раньше первой буквы и не кончится раньше конца
        // первого выравнивания: оно кончается раньше всех.
        let last = pos[q.len() - 1];
        let from = pos[0].max((last + 1).saturating_sub(q.len()));
        let mut best = score_positions(name, pos);
        // После первой буквы нет начал слов — выравнивание было бы тем же самым.
        let later_starts = name.f.starts.checked_shr(pos[0] as u32 + 1).unwrap_or(0) != 0;
        if bound > best && later_starts && align_starts(name, q, pos) {
            best = best.max(score_positions(name, pos));
        }
        // Одна буква: подстрока — то же, что вхождение в начале слова. Подстрока не
        // переходит через пробел: запрос длиннее участка без пробелов не поместится.
        if q.len() > 1 && q.len() <= name.f.run as usize && bound > best {
            if let Some(at) = self.substring(name, from, best) {
                best = best.max(self.substring_score(name, at));
            }
        }
        Some(best)
    }
}

/// Вес совпадения запроса с текстом по всем раскладкам; `None` — не совпало.
pub fn fuzzy_score(text: &str, query: &str) -> Option<i32> {
    let mut t = Vec::new();
    let f = index_name(text, &mut t);
    let name = Name { t: &t, f };
    let mut pos = Vec::new();
    layout_variants(query)
        .into_iter()
        .filter_map(|q| Needle::new(q).score(name, i32::MIN, &mut pos))
        .max()
}

/// Сколько символов имени попадёт в индекс; без разбора UTF-8.
fn head_chars(name: &str) -> usize {
    if name.len() >= FIND_NAME_CHARS * 4 {
        return FIND_NAME_CHARS;
    }
    let chars = name.bytes().filter(|&b| (b as i8) >= -0x40).count();
    chars.min(FIND_NAME_CHARS)
}

/// Сложенные имена страницы в порядке панели (сверху вниз) — одним массивом:
/// повторный запрос не трогает узлы сцены и не складывает имена заново. Живёт,
/// пока не сменились экземпляр сцены и её метка состава и имён.
#[derive(Debug, Default)]
pub struct LayerIndex {
    key: Option<(u64, u64)>,
    slots: Vec<Slot>,
    /// По одной записи на слот, в том же порядке.
    facts: Vec<NameFacts>,
    /// Символы имён подряд; длина каждого — в `facts`.
    chars: Vec<char>,
}

impl LayerIndex {
    fn sync(&mut self, scene: &Scene) {
        let key = (scene.generation(), scene.structure_stamp());
        if self.key == Some(key) {
            return;
        }
        // Новая сцена: сначала порядок и размер, чтобы вектор не рос и не было пика на
        // копирование; восьмая часть запаса — на правки имён. Правка той же сцены
        // размера почти не меняет, а лишний проход по узлам стоит времени первого
        // запроса после неё.
        let measure = self.key.is_none_or(|(generation, _)| generation != key.0);
        self.key = Some(key);
        self.slots.clear();
        self.facts.clear();
        self.chars.clear();
        let mut total = 0;
        let mut stack: Vec<Slot> = scene.roots().to_vec();
        while let Some(slot) = stack.pop() {
            if scene.is_draft_slot(slot) {
                continue;
            }
            let n = scene.at(slot);
            if measure {
                total += head_chars(&n.props.name);
            } else {
                let f = index_name(&n.props.name, &mut self.chars);
                self.facts.push(f);
            }
            self.slots.push(slot);
            // Дети — в прямом порядке: верхний (последний) снимается первым.
            stack.extend(n.children.iter().copied());
        }
        if measure {
            self.facts.reserve(self.slots.len() + self.slots.len() / 8);
            self.chars.reserve(total + total / 8);
            for &slot in &self.slots {
                let f = index_name(&scene.at(slot).props.name, &mut self.chars);
                self.facts.push(f);
            }
        }
    }

    /// Сколько памяти занято под индекс, в байтах (с запасом векторов).
    pub fn heap_bytes(&self) -> usize {
        self.slots.capacity() * size_of::<Slot>()
            + self.facts.capacity() * size_of::<NameFacts>()
            + self.chars.capacity() * size_of::<char>()
    }

    /// Слои страницы `scene` по нечёткому запросу: не больше `limit` (и
    /// `MAX_FIND_LIMIT`), лучшие первыми, при равном весе — в порядке панели
    /// слоёв (сверху вниз). Пустой запрос — пусто. Черновики жеста не ищутся.
    pub fn find(&mut self, scene: &Scene, query: &str, limit: usize) -> Vec<FoundLayer> {
        let limit = limit.min(MAX_FIND_LIMIT);
        let needles: Vec<Needle> = layout_variants(query)
            .into_iter()
            .map(Needle::new)
            .collect();
        if needles.is_empty() || limit == 0 {
            return Vec::new();
        }
        self.sync(scene);
        // Вершина кучи — худший из лучших. Имена идут в порядке панели, так что
        // новое имя с тем же весом проигрывает: войти может только вес выше `floor`.
        let mut top: BinaryHeap<(Reverse<i32>, u32)> = BinaryHeap::with_capacity(limit + 1);
        let mut floor = i32::MIN;
        let mut pos = Vec::new();
        let mut at = 0;
        for (i, &f) in self.facts.iter().enumerate() {
            let end = at + f.len as usize;
            let name = Name {
                t: &self.chars[at..end],
                f,
            };
            at = end;
            let mut best = floor;
            for needle in &needles {
                if let Some(s) = needle.score(name, best, &mut pos) {
                    best = best.max(s);
                }
            }
            if best > floor {
                if top.len() == limit {
                    top.pop();
                }
                top.push((Reverse(best), i as u32));
                if top.len() == limit {
                    floor = top.peek().map_or(floor, |h| h.0 .0);
                }
            }
        }
        top.into_sorted_vec()
            .into_iter()
            .filter_map(|(_, i)| scene.node_at(self.slots[i as usize]))
            .map(|n| FoundLayer {
                id: n.id,
                name: n.props.name.chars().take(MAX_NAME_CHARS).collect(),
                kind: n.props.kind,
            })
            .collect()
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
    /// как «Показать выделение». Скрытый (сам или через предка) — только выделение.
    /// Слоя нет на странице — `false`, ничего не меняется.
    pub fn reveal_layer(&mut self, id: NodeId) -> bool {
        if self
            .scene
            .slot(id)
            .is_none_or(|slot| self.scene.is_draft_slot(slot))
        {
            return false;
        }
        self.select(&[id]);
        // Слот — после выделения: оно может завершить правку и пересобрать сцену.
        let shown = self.scene.slot(id).filter(|&s| self.scene.is_visible(s));
        if let Some(b) = shown.and_then(|slot| world_box(&self.scene, slot)) {
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Быстрые пути свёртки обязаны совпадать с определением на каждом символе.
    #[test]
    fn fast_fold_paths_match_unicode() {
        for c in (0..=char::MAX as u32).filter_map(char::from_u32) {
            let mut want = Vec::new();
            fold(c, &mut want);
            let entry = FOLD.get(c as usize).copied().unwrap_or(NOT_IN_TABLE);
            if entry & NOT_IN_TABLE == 0 {
                let l = char::from_u32((entry & FOLDED_BITS) as u32).unwrap();
                assert_eq!(want, [l], "{c:?}");
                assert_eq!(entry & ALNUM != 0, is_alnum(l), "{c:?}");
                assert_eq!(entry & SPACE != 0, l.is_whitespace(), "{c:?}");
            }
            if is_caseless_letter(c) {
                assert_eq!(want, [c], "{c:?}");
                assert!(is_alnum(c) && !c.is_whitespace(), "{c:?}");
            }
        }
    }
}
