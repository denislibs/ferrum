//! Направляющие (спека подсистемы 2, раздел 9): ключ `guides` у страницы и у
//! фрейма — список `{ axis: "x" | "y", offset }`. Значение неизменяемое и
//! заменяется целиком, как заливки: так совместная правка и отмена проще, а узлов
//! в Loro не прибавляется. Смещение у страницы — от нуля страницы, у фрейма — от
//! его левого верхнего угла в его координатах.
//!
//! Данные из файла не доверенные: неверные записи (ось, нечисловое смещение, вне
//! предела, повторы) отбрасываются поодиночке, сверх `MAX_GUIDES` — не читаются.

use std::collections::HashSet;

use loro::LoroValue;

use crate::codec::{as_f64, as_str, field, obj};
use crate::error::{DocError, DocResult};

/// Наибольшее число направляющих у одного узла.
pub const MAX_GUIDES: usize = 1000;
/// Предел смещения по модулю.
pub const MAX_GUIDE_OFFSET: f64 = 1e9;

/// Ось, вдоль которой считается смещение: `X` — вертикальная линия на отметке x,
/// `Y` — горизонтальная на отметке y.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GuideAxis {
    X,
    Y,
}

impl GuideAxis {
    pub fn as_str(self) -> &'static str {
        match self {
            GuideAxis::X => "x",
            GuideAxis::Y => "y",
        }
    }

    pub fn parse(s: &str) -> Option<GuideAxis> {
        match s {
            "x" => Some(GuideAxis::X),
            "y" => Some(GuideAxis::Y),
            _ => None,
        }
    }
}

/// Одна направляющая.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Guide {
    pub axis: GuideAxis,
    pub offset: f64,
}

impl Guide {
    pub fn new(axis: GuideAxis, offset: f64) -> Guide {
        Guide { axis, offset }
    }

    /// Смещение конечно и в пределе.
    pub fn is_valid(&self) -> bool {
        self.offset.is_finite() && self.offset.abs() <= MAX_GUIDE_OFFSET
    }
}

/// Проверка списка, который пишет правка: сверх предела или с неверным смещением —
/// отказ целиком (`DocError::InvalidGuides`).
pub fn check_guides(guides: &[Guide]) -> DocResult<()> {
    if guides.len() > MAX_GUIDES {
        return Err(DocError::InvalidGuides(format!(
            "больше {MAX_GUIDES} на один узел"
        )));
    }
    if guides.iter().any(|g| !g.is_valid()) {
        return Err(DocError::InvalidGuides(
            "смещение должно быть конечным и не больше 1e9 по модулю".into(),
        ));
    }
    Ok(())
}

/// Без повторов (та же ось и то же смещение), первые на месте; `-0` — как `0`.
pub fn dedup_guides(guides: impl IntoIterator<Item = Guide>) -> Vec<Guide> {
    let mut seen = HashSet::new();
    guides
        .into_iter()
        .map(|g| Guide::new(g.axis, g.offset + 0.0))
        .filter(|g| seen.insert((g.axis, g.offset.to_bits())))
        .collect()
}

pub(crate) fn guides_value(guides: &[Guide]) -> LoroValue {
    LoroValue::from(
        guides
            .iter()
            .map(|g| {
                obj(vec![
                    ("axis", g.axis.as_str().into()),
                    ("offset", g.offset.into()),
                ])
            })
            .collect::<Vec<_>>(),
    )
}

/// Список как есть, без проверок: так тесты пишут неверные данные чужого файла.
pub(crate) fn raw_value(raw: &[(&str, f64)]) -> LoroValue {
    LoroValue::from(
        raw.iter()
            .map(|(axis, offset)| obj(vec![("axis", (*axis).into()), ("offset", (*offset).into())]))
            .collect::<Vec<_>>(),
    )
}

fn read_guide(v: &LoroValue) -> Option<Guide> {
    let axis = GuideAxis::parse(&as_str(field(v, "axis")?)?)?;
    let guide = Guide::new(axis, as_f64(field(v, "offset")?)?);
    guide.is_valid().then_some(guide)
}

/// Список из файла: неверные записи и повторы отброшены, не больше `MAX_GUIDES`.
/// Обход останавливается, набрав предел верных записей; список из одного мусора
/// проходится целиком (его размер ограничен файлом).
pub(crate) fn read_guides(v: &LoroValue) -> Vec<Guide> {
    let LoroValue::List(list) = v else {
        return Vec::new();
    };
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for item in list.iter() {
        if out.len() == MAX_GUIDES {
            break;
        }
        if let Some(g) = read_guide(item) {
            let g = Guide::new(g.axis, g.offset + 0.0);
            if seen.insert((g.axis, g.offset.to_bits())) {
                out.push(g);
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn raw(axis: &str, offset: f64) -> LoroValue {
        obj(vec![("axis", axis.into()), ("offset", offset.into())])
    }

    #[test]
    fn reading_drops_bad_entries_one_by_one() {
        let list = LoroValue::from(vec![
            raw("x", 10.0),
            raw("z", 1.0),
            raw("y", f64::NAN),
            raw("y", f64::INFINITY),
            raw("y", 1e9 + 1.0),
            raw("y", -1e9),
            raw("x", 10.0),
            "мусор".into(),
        ]);
        assert_eq!(
            read_guides(&list),
            [
                Guide::new(GuideAxis::X, 10.0),
                Guide::new(GuideAxis::Y, -1e9)
            ]
        );
        assert!(read_guides(&LoroValue::from(1.0)).is_empty());
    }

    #[test]
    fn reading_stops_at_the_limit() {
        let list = LoroValue::from(
            (0..MAX_GUIDES + 50)
                .map(|i| raw("x", i as f64))
                .collect::<Vec<_>>(),
        );
        let read = read_guides(&list);
        assert_eq!(read.len(), MAX_GUIDES);
        assert_eq!(read[MAX_GUIDES - 1].offset, (MAX_GUIDES - 1) as f64);
    }

    #[test]
    fn writing_is_refused_whole() {
        let ok = vec![Guide::new(GuideAxis::X, 1.0); MAX_GUIDES];
        assert!(check_guides(&ok).is_ok());
        let too_many = vec![Guide::new(GuideAxis::X, 1.0); MAX_GUIDES + 1];
        assert!(matches!(
            check_guides(&too_many),
            Err(DocError::InvalidGuides(_))
        ));
        for bad in [f64::NAN, f64::INFINITY, -f64::INFINITY, 1.5e9] {
            let list = [Guide::new(GuideAxis::Y, 0.0), Guide::new(GuideAxis::X, bad)];
            assert!(matches!(
                check_guides(&list),
                Err(DocError::InvalidGuides(_))
            ));
        }
    }

    #[test]
    fn dedup_keeps_first_and_folds_negative_zero() {
        let got = dedup_guides([
            Guide::new(GuideAxis::X, 0.0),
            Guide::new(GuideAxis::X, -0.0),
            Guide::new(GuideAxis::Y, 0.0),
        ]);
        assert_eq!(got.len(), 2);
        assert!(got[0].offset.is_sign_positive());
    }
}
