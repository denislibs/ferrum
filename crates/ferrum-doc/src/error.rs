use thiserror::Error;

#[derive(Debug, Error)]
pub enum DocError {
    #[error("Некорректный идентификатор: {0}")]
    InvalidId(String),
    #[error("Некорректный цвет: {0}. Ожидается #rrggbb или #rrggbbaa")]
    InvalidColor(String),
    #[error("Узел {0} не найден или удалён")]
    NodeNotFound(String),
    #[error("Страница {0} не найдена")]
    PageNotFound(String),
    #[error("Нельзя удалить единственную страницу")]
    LastPage,
    #[error("Узел {0} хранит неизвестный тип «{1}»")]
    UnknownKind(String, String),
    /// Сеть узла `vector` из файла не прошла проверку: узел скрывается, как узел
    /// неизвестного типа, и документ открывается только для чтения.
    #[error("Узел {0}: {1}")]
    InvalidVector(String, crate::vector::VectorError),
    #[error("Вектор длиннее 100 000 точек — правка не применена")]
    VectorTooLarge,
    #[error("Нельзя переместить узел внутрь самого себя")]
    CyclicMove,
    #[error("Документ открыт только для чтения")]
    ReadOnly,
    #[error("Это не файл Ferrum")]
    NotAFerrumFile,
    #[error("Файл создан в более новой версии формата ({0}). Обновите Ferrum")]
    UnsupportedFormat(u32),
    #[error("Файл повреждён: {0}")]
    Corrupted(String),
    #[error("Узел {0} — не текст")]
    NotText(String),
    #[error("Текст длиннее 100 000 символов — правка не применена")]
    TextTooLong,
    #[error("В тексте больше 10 000 участков со стилями — правка не применена")]
    TooManyTextRuns,
    #[error("Позиция {0} вне текста длиной {1}")]
    TextRange(usize, usize),
    #[error("Ошибка Loro: {0}")]
    Loro(#[from] loro::LoroError),
    #[error("Ошибка экспорта Loro: {0}")]
    LoroEncode(#[from] loro::LoroEncodeError),
    #[error("Не удалось откатить незавершённое изменение ({rollback}); исходная ошибка: {original}. Страница может быть в несогласованном состоянии — откройте файл заново")]
    RollbackFailed {
        original: Box<DocError>,
        rollback: Box<DocError>,
    },
}

pub type DocResult<T> = Result<T, DocError>;
