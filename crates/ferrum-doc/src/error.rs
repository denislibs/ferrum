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
    #[error("Узел {0} хранит неизвестный тип «{1}»")]
    UnknownKind(String, String),
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
