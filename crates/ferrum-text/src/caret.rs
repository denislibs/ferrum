//! Курсор и выделение по раскладке (спека подсистемы 3, раздел 7): позиция по
//! точке, прямоугольник курсора и выделения, ход по кластерам, строкам и словам.
//! Позиции — символы Unicode содержимого узла, как у `LoroText`; координаты —
//! узла (0, 0 — левый верхний угол рамки). Текст справа налево не поддерживается
//! (спека 1: не входит).
//!
//! Позиция на переносе строки — одна и та же для конца верхней строки и начала
//! нижней; `upstream` выбирает верхнюю (курсор после щелчка за концом строки и
//! после End), иначе — нижнюю.

use std::ops::Range;

use kurbo::{Point, Rect};

use crate::layout::{CaretLine, TextLayout};

/// Позиция курсора: символ и сторона переноса строки.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Caret {
    pub index: usize,
    /// На переносе строки — в конце верхней строки, а не в начале нижней.
    pub upstream: bool,
}

impl Caret {
    pub fn at(index: usize) -> Caret {
        Caret {
            index,
            upstream: false,
        }
    }
}

/// Сколько добавить к выделению за концом абзаца (перевод строки виден как
/// узкая полоска, как в Figma), в долях высоты строки.
const NEWLINE_SELECTION: f64 = 0.25;

impl CaretLine {
    /// x позиции `index` на строке (за её пределами — ближайший край).
    pub fn x_of(&self, index: usize) -> f64 {
        match self.stops.binary_search_by_key(&index, |s| s.0) {
            Ok(i) => self.stops[i].1,
            Err(0) => self.stops.first().map_or(0.0, |s| s.1),
            // Внутри кластера (несколько символов) — его левый край.
            Err(i) => self.stops[i - 1].1,
        }
    }

    /// Ближайшая к `x` позиция курсора на строке.
    pub fn nearest(&self, x: f64) -> usize {
        let mut best = self.start;
        let mut dist = f64::INFINITY;
        for (c, sx) in &self.stops {
            let d = (sx - x).abs();
            if d < dist {
                (best, dist) = (*c, d);
            }
        }
        best
    }
}

impl TextLayout {
    /// Номер строки позиции: `index` на переносе с `upstream` — верхняя строка.
    pub fn caret_line(&self, caret: Caret) -> Option<usize> {
        let n = self.carets.len();
        if n == 0 {
            return None;
        }
        for (i, l) in self.carets.iter().enumerate() {
            if caret.index < l.end
                || (caret.index == l.end && (l.hard || caret.upstream || i + 1 == n))
            {
                return Some(i);
            }
        }
        Some(n - 1)
    }

    /// Отрезок курсора: x, верх и низ строки.
    pub fn caret_rect(&self, caret: Caret) -> Option<(f64, f64, f64)> {
        let l = &self.carets[self.caret_line(caret)?];
        Some((l.x_of(caret.index), l.top, l.bottom))
    }

    /// Позиция по точке узла: строка — по высоте (выше первой — первая, ниже
    /// последней — последняя), позиция — ближайшая по x.
    pub fn hit(&self, p: Point) -> Caret {
        let n = self.carets.len();
        let Some(i) = self
            .carets
            .iter()
            .position(|l| p.y < l.bottom)
            .or(n.checked_sub(1))
        else {
            return Caret::default();
        };
        let l = &self.carets[i];
        let index = l.nearest(p.x);
        Caret {
            index,
            upstream: index == l.end && !l.hard && i + 1 < n,
        }
    }

    /// Прямоугольники выделения диапазона — по строке на строку.
    pub fn selection_rects(&self, range: Range<usize>) -> Vec<Rect> {
        let mut out = Vec::new();
        if range.is_empty() {
            return out;
        }
        for l in &self.carets {
            if l.end < range.start || l.start >= range.end {
                continue;
            }
            // Переход строки переносом принадлежит нижней строке.
            if l.end == range.start && !l.hard {
                continue;
            }
            let x0 = l.x_of(range.start.max(l.start));
            let mut x1 = l.x_of(range.end.min(l.end));
            if l.hard && range.end > l.end {
                x1 += (l.bottom - l.top) * NEWLINE_SELECTION;
            }
            if x1 > x0 {
                out.push(Rect::new(x0, l.top, x1, l.bottom));
            }
        }
        out
    }

    /// Соседняя позиция курсора (граница кластера) после `index`; в конце —
    /// `index`.
    pub fn next_stop(&self, index: usize) -> usize {
        self.carets
            .iter()
            .flat_map(|l| l.stops.iter())
            .map(|s| s.0)
            .find(|c| *c > index)
            .unwrap_or(index)
    }

    /// Соседняя позиция курсора до `index`; в начале — `index`.
    pub fn prev_stop(&self, index: usize) -> usize {
        self.carets
            .iter()
            .rev()
            .flat_map(|l| l.stops.iter().rev())
            .map(|s| s.0)
            .find(|c| *c < index)
            .unwrap_or(index)
    }

    /// Позиция на строке выше (`delta < 0`) или ниже, ближайшая к `x`; за первой
    /// или последней строкой — `None`.
    pub fn vertical(&self, caret: Caret, x: f64, delta: isize) -> Option<Caret> {
        let i = self.caret_line(caret)? as isize + delta;
        if i < 0 || i as usize >= self.carets.len() {
            return None;
        }
        let l = &self.carets[i as usize];
        let index = l.nearest(x);
        Some(Caret {
            index,
            upstream: index == l.end && !l.hard,
        })
    }

    /// Начало и конец видимой строки позиции (Home/End, Cmd/Ctrl+←/→); у конца
    /// перенесённой строки — `upstream`.
    pub fn line_bounds(&self, caret: Caret) -> Option<(Caret, Caret)> {
        let l = &self.carets[self.caret_line(caret)?];
        Some((
            Caret::at(l.start),
            Caret {
                index: l.end,
                upstream: !l.hard,
            },
        ))
    }
}

/// Буква слова: буквы, цифры и подчёркивание.
fn is_word(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// Класс символа для выделения двойным щелчком: слово, пробелы, остальное.
fn class(c: char) -> u8 {
    if is_word(c) {
        0
    } else if c.is_whitespace() && c != '\n' {
        1
    } else {
        2
    }
}

/// Слово под позицией (двойной щелчок): подряд идущие символы одного класса —
/// буквы, пробелы или знаки; перевод строки — пусто.
pub fn word_range(text: &[char], at: usize) -> Range<usize> {
    let len = text.len();
    if len == 0 {
        return 0..0;
    }
    // Справа от конца слова (щелчок за последней буквой) — то же слово.
    let mut i = at.min(len - 1);
    if (text[i] == '\n' || !is_word(text[i])) && i > 0 && is_word(text[i - 1]) {
        i -= 1;
    }
    if text[i] == '\n' {
        return at..at;
    }
    let k = class(text[i]);
    let mut a = i;
    while a > 0 && text[a - 1] != '\n' && class(text[a - 1]) == k {
        a -= 1;
    }
    let mut b = i + 1;
    while b < len && text[b] != '\n' && class(text[b]) == k {
        b += 1;
    }
    a..b
}

/// Конец следующего слова (Alt/Option+→ на macOS, Ctrl+→ на остальных).
pub fn next_word_end(text: &[char], at: usize) -> usize {
    let len = text.len();
    let mut i = at.min(len);
    while i < len && !is_word(text[i]) {
        i += 1;
    }
    while i < len && is_word(text[i]) {
        i += 1;
    }
    i
}

/// Начало предыдущего слова (Alt/Option+←, Ctrl+←).
pub fn prev_word_start(text: &[char], at: usize) -> usize {
    let mut i = at.min(text.len());
    while i > 0 && !is_word(text[i - 1]) {
        i -= 1;
    }
    while i > 0 && is_word(text[i - 1]) {
        i -= 1;
    }
    i
}

/// Абзац позиции без его перевода строки (тройной щелчок, Alt+↑/↓ на macOS).
pub fn paragraph_range(text: &[char], at: usize) -> Range<usize> {
    let len = text.len();
    let at = at.min(len);
    let mut a = at;
    while a > 0 && text[a - 1] != '\n' {
        a -= 1;
    }
    let mut b = at;
    while b < len && text[b] != '\n' {
        b += 1;
    }
    a..b
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chars(s: &str) -> Vec<char> {
        s.chars().collect()
    }

    #[test]
    fn words_and_paragraphs() {
        let t = chars("Привет, мир!\nвторой  абзац\n");
        assert_eq!(word_range(&t, 2), 0..6);
        // Щелчок сразу за словом — то же слово; по пробелам — пробелы.
        assert_eq!(word_range(&t, 6), 0..6);
        assert_eq!(word_range(&t, 20), 19..21);
        assert_eq!(next_word_end(&t, 0), 6);
        assert_eq!(next_word_end(&t, 6), 11);
        assert_eq!(prev_word_start(&t, 11), 8);
        assert_eq!(prev_word_start(&t, 8), 0);
        assert_eq!(paragraph_range(&t, 15), 13..26);
        assert_eq!(paragraph_range(&t, 12), 0..12);
        // Пустой абзац — пустой диапазон в его начале.
        let e = chars("а\n\nб\n");
        assert_eq!(paragraph_range(&e, 2), 2..2);
        assert_eq!(word_range(&e, 2), 2..2);
    }
}
