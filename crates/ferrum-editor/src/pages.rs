//! Страницы из панели «Страницы» (этап 5): создать, переименовать, удалить,
//! цвет фона. Как и в `ferrum-doc`, операции со страницами не попадают в историю
//! отмены. Имена из интерфейса не доверенные: `clean_name` убирает управляющие
//! символы и ограничивает длину.

use ferrum_doc::{Color, DocResult, PageId};

use crate::editor::{Editor, EditorEvent};

/// Наибольшая длина имени слоя, страницы и файла в символах.
pub const MAX_NAME_CHARS: usize = 256;

/// Имя из поля ввода: без управляющих символов (перевод строки, табуляция — в
/// пробел), без пробелов по краям, не длиннее `MAX_NAME_CHARS`. Пустое — `None`:
/// интерфейс возвращает прежнее имя, как в Figma.
pub fn clean_name(name: &str) -> Option<String> {
    let s: String = name
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect();
    let s: String = s.trim().chars().take(MAX_NAME_CHARS).collect();
    let s = s.trim_end().to_string();
    (!s.is_empty()).then_some(s)
}

impl Editor {
    /// Новая страница в конце списка с первым свободным именем «Страница N»;
    /// она сразу становится текущей.
    pub fn create_page(&mut self) -> DocResult<PageId> {
        let names: Vec<String> = self.doc.pages().into_iter().map(|(_, n)| n).collect();
        let mut n = names.len() + 1;
        while names.iter().any(|x| *x == format!("Страница {n}")) {
            n += 1;
        }
        let id = self.doc.add_page(&format!("Страница {n}"))?;
        self.set_page(id)?;
        Ok(id)
    }

    /// Переименование; пустое имя ничего не меняет.
    pub fn rename_page(&mut self, id: PageId, name: &str) -> DocResult<()> {
        // Шаг правки — после шага открытого пути пера (см. `begin_doc_edit`).
        self.finish_pen_path();
        match clean_name(name) {
            Some(name) => self.doc.rename_page(id, &name),
            None => Ok(()),
        }
    }

    /// Удаляет страницу. Если она текущая, сначала переключает на соседнюю
    /// (следующую, у последней — предыдущую). Единственную удалить нельзя.
    pub fn delete_page(&mut self, id: PageId) -> DocResult<()> {
        let pages = self.doc.pages();
        if pages.len() <= 1 {
            return Err(ferrum_doc::DocError::LastPage);
        }
        self.finish_pen_path();
        if id == self.page {
            let at = pages.iter().position(|(p, _)| *p == id).unwrap_or(0);
            let next = if at + 1 < pages.len() { at + 1 } else { at - 1 };
            self.set_page(pages[next].0)?;
        }
        self.doc.delete_page(id)
    }

    /// Имя документа (шапка панели, имя файла); пустое ничего не меняет.
    /// Возвращает имя после правки.
    pub fn set_document_name(&mut self, name: &str) -> DocResult<String> {
        self.finish_pen_path();
        if let Some(name) = clean_name(name) {
            self.doc.set_name(&name)?;
        }
        Ok(self.doc.name())
    }

    /// Цвет фона текущей страницы; `None` — как у холста в теме.
    pub fn page_background(&self) -> Option<Color> {
        self.doc.page_background(self.page)
    }

    /// Задаёт фон страницы. Холст перерисовывается, если страница текущая.
    pub fn set_page_background(&mut self, id: PageId, color: Option<Color>) -> DocResult<()> {
        self.finish_pen_path();
        self.doc.set_page_background(id, color)?;
        if id == self.page {
            self.out.events.push(EditorEvent::NeedsRedraw);
        }
        Ok(())
    }
}
