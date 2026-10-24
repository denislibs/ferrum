//! Политика ошибок показа кадра: что делать, когда поверхность не приняла кадр.
//! Отдельно от браузерного кода, чтобы проверяться обычными тестами.
//!
//! Пропуск кадра (таймаут, устаревшая конфигурация) обычно проходит сам: кадр
//! рисуется заново. Но если пропуски идут подряд, вечная перерисовка только жжёт
//! процессор, а пользователь не узнаёт причину. Кадр больше предела устройства —
//! пауза до смены размера: окно уменьшили — рисуем снова. Любая другая ошибка
//! постоянна: о ней сообщается один раз, и поверхность перестаёт рисовать.
//!
//! Ошибки WebGPU вне вызовов (`uncapturederror`) приходят сами по себе и часто
//! повторяются каждый кадр: каждый текст — одно сообщение (`SoftErrors`), а слишком
//! много ошибок — признак сломанной поверхности, она останавливается.

use std::collections::HashSet;

/// Сколько пропусков кадра подряд считаются ошибкой.
pub const MAX_SKIPS: u32 = 3;
/// Сколько ошибок WebGPU вне вызовов терпим, прежде чем остановить поверхность.
/// Заодно предел множества уже показанных текстов.
pub const MAX_SOFT_ERRORS: u32 = 30;

/// Почему кадр не показан.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PresentError {
    /// Временно: кадр можно нарисовать снова.
    Skipped(String),
    /// Кадр больше предела устройства: рисовать снова, когда размер изменится.
    TooLarge(String),
    /// Постоянно: повтор не поможет.
    Failed(String),
}

/// Что делать дальше.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    /// Нарисовать кадр снова.
    Redraw,
    /// Не просить кадров (следующий придёт с вводом); сообщение — если оно новое.
    Pause(Option<String>),
    /// Поверхность больше не рисует; сообщение — если оно новое.
    Dead(Option<String>),
}

#[derive(Debug, Default)]
pub struct PresentWatch {
    skips: u32,
    reported: HashSet<String>,
    /// О слишком большом кадре уже сообщили (размер при росте окна меняется,
    /// а сообщение нужно одно).
    too_large: bool,
}

impl PresentWatch {
    /// Кадр показан: счётчики сбрасываются.
    pub fn ok(&mut self) {
        self.skips = 0;
        self.reported.clear();
        self.too_large = false;
    }

    pub fn error(&mut self, e: PresentError) -> Action {
        match e {
            PresentError::Skipped(m) => {
                self.skips += 1;
                if self.skips < MAX_SKIPS {
                    return Action::Redraw;
                }
                // Без числа в тексте: повторы одного и того же не плодят событий.
                Action::Pause(self.fresh(format!("Кадр не показан несколько раз подряд: {m}")))
            }
            PresentError::TooLarge(m) => {
                let first = !std::mem::replace(&mut self.too_large, true);
                Action::Pause(first.then(|| format!("Кадр не показан: {m}; уменьшите окно")))
            }
            PresentError::Failed(m) => {
                Action::Dead(self.fresh(format!("Не удалось показать кадр: {m}")))
            }
        }
    }

    /// Сообщение, которого ещё не было.
    fn fresh(&mut self, message: String) -> Option<String> {
        self.reported.insert(message.clone()).then_some(message)
    }
}

/// Что делать с ошибкой WebGPU вне вызовов.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Soft {
    /// Новый текст: сообщить, рисование продолжается.
    Report(String),
    /// Уже было (или поверхность уже остановлена): ничего.
    Quiet,
    /// Ошибок слишком много: сообщить и остановить поверхность.
    Dead(String),
}

#[derive(Debug, Default)]
pub struct SoftErrors {
    total: u32,
    seen: HashSet<String>,
    dead: bool,
}

impl SoftErrors {
    pub fn error(&mut self, message: String) -> Soft {
        if self.dead {
            return Soft::Quiet;
        }
        self.total += 1;
        if self.total > MAX_SOFT_ERRORS {
            self.dead = true;
            self.seen.clear();
            return Soft::Dead(format!(
                "Слишком много ошибок WebGPU (больше {MAX_SOFT_ERRORS}), рисование остановлено. Последняя: {message}"
            ));
        }
        if self.seen.insert(message.clone()) {
            Soft::Report(message)
        } else {
            Soft::Quiet
        }
    }
}
