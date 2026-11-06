//! Перенос дельты текста через чужую правку: шаг отмены записан относительно текста, каким он был,
//! а другой человек с тех пор вставил или удалил символы (спека подсистемы 4, раздел 8).
//! Обычное преобразование операций: `transform(a, b, side)` — как применить `a` после `b`, если обе
//! написаны к одному и тому же тексту. Чтобы оба порядка применения давали один текст, две вставки в
//! одно место решает сторона, и у двух встречных преобразований она разная.

use loro::TextDelta;

/// Длина операции в символах исходного текста; вставка исходного текста не занимает.
struct Cursor<'a> {
    ops: &'a [TextDelta],
    i: usize,
    /// Сколько символов текущей операции (retain или delete) уже пройдено.
    used: usize,
}

impl<'a> Cursor<'a> {
    fn new(ops: &'a [TextDelta]) -> Cursor<'a> {
        Cursor { ops, i: 0, used: 0 }
    }

    fn peek(&self) -> Option<&'a TextDelta> {
        self.ops.get(self.i)
    }

    /// Остаток текущей операции по исходному тексту.
    fn left(&self) -> usize {
        match self.peek() {
            Some(TextDelta::Retain { retain, .. }) => retain - self.used,
            Some(TextDelta::Delete { delete }) => delete - self.used,
            Some(TextDelta::Insert { .. }) => 0,
            None => usize::MAX,
        }
    }

    fn advance(&mut self, n: usize) {
        if self.peek().is_none() {
            return;
        }
        self.used += n;
        if self.left() == 0 {
            self.i += 1;
            self.used = 0;
        }
    }
}

fn push(out: &mut Vec<TextDelta>, op: TextDelta) {
    match (out.last_mut(), &op) {
        (
            Some(TextDelta::Retain {
                retain,
                attributes: None,
            }),
            TextDelta::Retain {
                retain: n,
                attributes: None,
            },
        ) => {
            *retain += n;
        }
        (Some(TextDelta::Delete { delete }), TextDelta::Delete { delete: n }) => *delete += n,
        _ => out.push(op),
    }
}

/// Кто из двух вставок в одно место стоит раньше: `a` или `b` из `transform(a, b, side)`.
#[derive(Clone, Copy, PartialEq)]
pub(crate) enum Tie {
    AFirst,
    BFirst,
}

/// `a` после `b`. Обе — к одному тексту, хвост после последней операции — как `retain`.
pub(crate) fn transform(a: &[TextDelta], b: &[TextDelta], tie: Tie) -> Vec<TextDelta> {
    let (mut x, mut y) = (Cursor::new(a), Cursor::new(b));
    let mut out = Vec::new();
    loop {
        if tie == Tie::AFirst {
            if let Some(op @ TextDelta::Insert { .. }) = x.peek() {
                push(&mut out, op.clone());
                x.i += 1;
                continue;
            }
        }
        // Вставка `b`: операция `a` её только перешагивает.
        if let Some(TextDelta::Insert { insert, .. }) = y.peek() {
            push(
                &mut out,
                TextDelta::Retain {
                    retain: insert.chars().count(),
                    attributes: None,
                },
            );
            y.i += 1;
            continue;
        }
        match x.peek() {
            None => break,
            Some(op @ TextDelta::Insert { .. }) => {
                push(&mut out, op.clone());
                x.i += 1;
            }
            Some(op) => {
                let n = x.left().min(y.left());
                let gone = matches!(y.peek(), Some(TextDelta::Delete { .. }));
                // Символы, которые чужая правка удалила, своей операции уже не нужны.
                if !gone {
                    match op {
                        TextDelta::Retain { attributes, .. } => push(
                            &mut out,
                            TextDelta::Retain {
                                retain: n,
                                attributes: attributes.clone(),
                            },
                        ),
                        TextDelta::Delete { .. } => push(&mut out, TextDelta::Delete { delete: n }),
                        TextDelta::Insert { .. } => unreachable!("вставка разобрана выше"),
                    }
                }
                x.advance(n);
                y.advance(n);
            }
        }
    }
    // Хвостовой `retain` без разметки ничего не делает.
    if matches!(
        out.last(),
        Some(TextDelta::Retain {
            attributes: None,
            ..
        })
    ) {
        out.pop();
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use loro::LoroDoc;

    fn ret(n: usize) -> TextDelta {
        TextDelta::Retain {
            retain: n,
            attributes: None,
        }
    }
    fn ins(s: &str) -> TextDelta {
        TextDelta::Insert {
            insert: s.into(),
            attributes: None,
        }
    }
    fn del(n: usize) -> TextDelta {
        TextDelta::Delete { delete: n }
    }

    fn apply(start: &str, ops: &[&[TextDelta]]) -> String {
        let doc = LoroDoc::new();
        let t = doc.get_text("t");
        t.insert(0, start).unwrap();
        for d in ops {
            t.apply_delta(d).unwrap();
        }
        t.to_string()
    }

    #[test]
    fn deleting_own_insert_survives_foreign_insert_before_it() {
        // Текст «аб»: своя вставка «XY» в 1 дала «аXYб», чужая дописала «!» в начало.
        let undo = [ret(1), del(2)];
        let foreign = [ins("!")];
        let moved = transform(&undo, &foreign, Tie::BFirst);
        assert_eq!(apply("аXYб", &[&foreign, &moved]), "!аб");
    }

    #[test]
    fn foreign_insert_inside_deleted_range_stays() {
        let undo = [ret(1), del(2)];
        let foreign = [ret(2), ins("!")];
        let moved = transform(&undo, &foreign, Tie::BFirst);
        assert_eq!(apply("аXYб", &[&foreign, &moved]), "а!б");
    }

    #[test]
    fn foreign_delete_shrinks_own_delete() {
        let undo = [ret(1), del(3)];
        let foreign = [ret(2), del(5)];
        let moved = transform(&undo, &foreign, Tie::BFirst);
        assert_eq!(apply("0123456789", &[&foreign, &moved]), "0789");
    }

    #[test]
    fn insert_at_the_same_place_goes_after_foreign() {
        let own = [ret(1), ins("X")];
        let foreign = [ret(1), ins("Y")];
        let moved = transform(&own, &foreign, Tie::BFirst);
        assert_eq!(apply("аб", &[&foreign, &moved]), "аYXб");
    }

    /// Оба порядка применения дают один текст: без этого нижние шаги стека отмены приведены не к тому
    /// тексту, который есть на самом деле.
    #[test]
    fn both_orders_of_a_tie_give_the_same_text() {
        let own = [ret(1), ins("X")];
        let foreign = [ret(1), ins("Y")];
        let via_foreign = [&foreign[..], &transform(&own, &foreign, Tie::BFirst)];
        let via_own = [&own[..], &transform(&foreign, &own, Tie::AFirst)];
        assert_eq!(apply("аб", &via_foreign), apply("аб", &via_own));
    }

    #[test]
    fn no_tie_is_the_same_for_both_sides() {
        let own = [ret(1), del(1), ins("😀")];
        let foreign = [ret(3), ins("!")];
        assert_eq!(
            transform(&own, &foreign, Tie::AFirst),
            transform(&own, &foreign, Tie::BFirst)
        );
    }

    #[test]
    fn empty_foreign_keeps_delta_meaning() {
        let own = [ret(2), del(1)];
        assert_eq!(apply("абв", &[&transform(&own, &[], Tie::BFirst)]), "аб");
    }
}
