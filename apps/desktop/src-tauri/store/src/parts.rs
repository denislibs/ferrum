//! Части одного чтения файла для IPC. Чтение берёт метаданные, байты файла, сеанс и
//! пачки журнала одним вызовом под одной блокировкой библиотеки (иначе запись между
//! вызовами дала бы старые байты с новым поколением и без стёртых пачек), а байты
//! отдаются по одной части за вызов сырым ответом IPC: каждая часть — свой
//! `ArrayBuffer` в окне, без JSON и без копий.

use std::collections::VecDeque;

/// Чтение брошено (отказ IPC посреди чтения) или вытеснено новыми.
const LOST: &str = "Чтение файла прервано — откройте файл заново";

#[derive(Default)]
pub struct ReadParts {
    next: u64,
    open: VecDeque<(u64, VecDeque<Vec<u8>>)>,
}

impl ReadParts {
    /// Незабранных чтений не больше этого: брошенное не держит память вечно.
    pub const KEEP: usize = 2;

    /// Части чтения по порядку; ответ — номер чтения.
    pub fn put(&mut self, parts: Vec<Vec<u8>>) -> u64 {
        self.next += 1;
        if !parts.is_empty() {
            while self.open.len() >= Self::KEEP {
                self.open.pop_front();
            }
            self.open.push_back((self.next, parts.into()));
        }
        self.next
    }

    /// Следующая часть чтения; последняя закрывает его.
    pub fn take(&mut self, ticket: u64) -> Result<Vec<u8>, String> {
        let i = self
            .open
            .iter()
            .position(|(t, _)| *t == ticket)
            .ok_or_else(|| LOST.to_string())?;
        let part = self.open[i].1.pop_front().ok_or_else(|| LOST.to_string())?;
        if self.open[i].1.is_empty() {
            self.open.remove(i);
        }
        Ok(part)
    }
}
