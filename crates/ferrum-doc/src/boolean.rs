//! Булева операция (спека подсистемы 3, раздел 10): узел `boolean` — контейнер,
//! как группа, а его форма — результат операции над формами детей. Сама
//! операция — значение ключа `booleanOp` узла; результат документ не хранит, его
//! считает сцена.

/// Операция булевой группы, как в Figma: объединение, вычитание верхних слоёв из
/// нижнего, пересечение всех, исключение (нечётное число перекрытий).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum BooleanOp {
    #[default]
    Union,
    Subtract,
    Intersect,
    Exclude,
}

impl BooleanOp {
    pub const ALL: [BooleanOp; 4] = [
        BooleanOp::Union,
        BooleanOp::Subtract,
        BooleanOp::Intersect,
        BooleanOp::Exclude,
    ];

    /// Значение в документе, буфере обмена и протоколе.
    pub fn as_str(self) -> &'static str {
        match self {
            BooleanOp::Union => "union",
            BooleanOp::Subtract => "subtract",
            BooleanOp::Intersect => "intersect",
            BooleanOp::Exclude => "exclude",
        }
    }

    /// Только известные значения: данные из файла и буфера обмена не доверенные.
    pub fn parse(s: &str) -> Option<BooleanOp> {
        BooleanOp::ALL.into_iter().find(|op| op.as_str() == s)
    }

    /// Название операции в интерфейсе — им же называется новая булева группа.
    pub fn name(self) -> &'static str {
        match self {
            BooleanOp::Union => "Объединение",
            BooleanOp::Subtract => "Вычитание",
            BooleanOp::Intersect => "Пересечение",
            BooleanOp::Exclude => "Исключение",
        }
    }
}
