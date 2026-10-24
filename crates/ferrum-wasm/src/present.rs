//! Политика ошибок показа кадра: что делать, когда поверхность не приняла кадр.
//! Отдельно от браузерного кода, чтобы проверяться обычными тестами.
//!
//! Пропуск кадра (таймаут, устаревшая конфигурация) обычно проходит сам: кадр
//! рисуется заново. Но если пропуски идут подряд, вечная перерисовка только жжёт
//! процессор, а пользователь не узнаёт причину. Любая другая ошибка постоянна:
//! о ней сообщается один раз, и поверхность перестаёт рисовать.

use std::collections::HashSet;

/// Сколько пропусков кадра подряд считаются ошибкой.
pub const MAX_SKIPS: u32 = 3;

/// Почему кадр не показан.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PresentError {
    /// Временно: кадр можно нарисовать снова.
    Skipped(String),
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
}

impl PresentWatch {
    /// Кадр показан: счётчики сбрасываются.
    pub fn ok(&mut self) {
        self.skips = 0;
        self.reported.clear();
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
