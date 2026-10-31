//! Правки текста для редактора (подсистема 3, этап 2): фрагменты содержимого,
//! вставка участков со своей разметкой (буфер обмена), содержимое в патче
//! свойств (превью сцены и правка панели одним шагом), сравнение содержимого без
//! пустых шагов и текст сверх пределов в файле.

use ferrum_doc::*;

fn bold() -> TextAttrs {
    TextAttrs {
        style: Some("Bold".into()),
        ..Default::default()
    }
}

/// Документ с текстом «Привет, мир», где «Привет» — жирным.
fn doc_with_text() -> (Document, PageId, NodeId) {
    let mut doc = Document::new();
    let page = doc.pages()[0].0;
    let id = doc
        .transact(page, vec![], |tx| {
            let id = tx.create_node(None, None, NodeKind::Text)?;
            tx.set_text_content(id, &TextContent::plain("Привет, мир"))?;
            tx.format_text(id, 0..6, &bold(), &[])?;
            Ok(vec![id])
        })
        .unwrap()[0];
    (doc, page, id)
}

fn content(doc: &mut Document, page: PageId, id: NodeId) -> TextContent {
    doc.page(page)
        .unwrap()
        .props(id)
        .unwrap()
        .text
        .unwrap()
        .content
        .clone()
}

#[test]
fn slice_keeps_run_attrs_and_attrs_at_reads_one_char() {
    let (mut doc, page, id) = doc_with_text();
    let c = content(&mut doc, page, id);
    let part = c.slice(4..9);
    assert_eq!(part.len(), 2);
    assert_eq!((part[0].text.as_str(), &part[0].attrs), ("ет", &bold()));
    assert_eq!(part[1].text, ", м");
    assert!(part[1].attrs.is_empty());
    // За концом — обрезается, пустой диапазон — пусто.
    assert_eq!(c.slice(10..50)[0].text, "р\n");
    assert!(c.slice(3..3).is_empty());
    assert_eq!(c.attrs_at(0), &bold());
    assert!(c.attrs_at(6).is_empty());
    // За концом — разметка перевода строки.
    assert!(c.attrs_at(1000).is_empty());
}

#[test]
fn normalize_merges_equal_neighbours_and_drops_empty_runs() {
    let mut c = TextContent {
        runs: vec![
            TextRun {
                text: "а".into(),
                attrs: bold(),
            },
            TextRun {
                text: String::new(),
                attrs: TextAttrs::default(),
            },
            TextRun {
                text: "б".into(),
                attrs: bold(),
            },
            TextRun {
                text: "\n".into(),
                attrs: TextAttrs::default(),
            },
        ],
    };
    c.normalize();
    assert_eq!(c.runs.len(), 2);
    assert_eq!(c.runs[0].text, "аб");
}

#[test]
fn insert_runs_keeps_exact_attrs_and_undoes_in_one_step() {
    let (mut doc, page, id) = doc_with_text();
    let red = TextAttrs {
        fill: Some(Fill::solid(Color::rgba(255, 0, 0, 255))),
        ..Default::default()
    };
    let runs = vec![
        TextRun {
            text: "А".into(),
            attrs: red.clone(),
        },
        TextRun {
            text: "Б".into(),
            attrs: TextAttrs::default(),
        },
    ];
    // Вставка сразу за жирным «Привет»: участки — со своей разметкой, не с жирной.
    doc.transact(page, vec![], |tx| {
        tx.insert_runs(id, 6, &runs)?;
        Ok(vec![])
    })
    .unwrap();
    let c = content(&mut doc, page, id);
    assert_eq!(c.text(), "ПриветАБ, мир\n");
    assert_eq!(c.attrs_at(6), &red);
    assert!(c.attrs_at(7).is_empty());
    // За последним переводом строки — нельзя; пустые участки — правки нет.
    let err = doc
        .transact(page, vec![], |tx| {
            tx.insert_runs(id, 14, &runs)?;
            Ok(vec![])
        })
        .unwrap_err();
    assert!(matches!(err, DocError::TextRange(14, 14)));
    doc.undo().unwrap();
    assert_eq!(content(&mut doc, page, id).text(), "Привет, мир\n");
}

#[test]
fn content_in_patch_is_one_undo_step_with_props() {
    let (mut doc, page, id) = doc_with_text();
    let before = doc.page(page).unwrap().props(id).unwrap();
    let mut next = TextContent::plain("Новый");
    next.runs[0].attrs = bold();
    doc.transact(page, vec![], |tx| {
        tx.set_props(
            id,
            &PropsPatch {
                content: Some(next.clone()),
                w: Some(77.0),
                ..Default::default()
            },
        )?;
        Ok(vec![])
    })
    .unwrap();
    let after = doc.page(page).unwrap().props(id).unwrap();
    assert_eq!(after.w, 77.0);
    assert_eq!(after.text.as_ref().unwrap().content.text(), "Новый\n");
    // Патч применяется к свойствам так же, как в документе (превью сцены).
    let mut preview = before.clone();
    PropsPatch {
        content: Some(next),
        ..Default::default()
    }
    .apply_to(&mut preview);
    assert_eq!(
        preview.text.as_ref().unwrap().content.text(),
        after.text.as_ref().unwrap().content.text()
    );
    // Один шаг: его отмена возвращает и свойства, и содержимое; дальше — только
    // создание узла.
    doc.undo().unwrap();
    assert_eq!(doc.page(page).unwrap().props(id).unwrap(), before);
    doc.undo().unwrap();
    assert!(!doc.can_undo());
}

#[test]
fn same_content_split_differently_makes_no_step() {
    let (mut doc, page, id) = doc_with_text();
    // «Привет» жирным — двумя участками той же разметки и пустой участок.
    let mut split = content(&mut doc, page, id);
    split.runs.remove(0);
    for (i, part) in ["ет", "", "Прив"].into_iter().enumerate() {
        split.runs.insert(
            0,
            TextRun {
                text: part.into(),
                attrs: if i == 1 { TextAttrs::default() } else { bold() },
            },
        );
    }
    assert_eq!(split.text(), "Привет, мир\n");
    doc.transact(page, vec![], |tx| {
        tx.set_text_content(id, &split)?;
        Ok(vec![])
    })
    .unwrap();
    // Шаг только один — создание: после его отмены отменять нечего.
    doc.undo().unwrap();
    assert!(!doc.can_undo());
}

#[test]
fn oversized_text_in_file_makes_page_read_only() {
    let (mut doc, page, id) = doc_with_text();
    let p = doc.page(page).unwrap();
    assert!(!p.has_oversized_text());
    p.force_text_for_tests(id, &"я".repeat(MAX_TEXT_CHARS + 1));
    // Снапшот без кэша сцены — путь чтения из Loro.
    let snapshot = p.export_snapshot().unwrap();
    let reopened = PageDoc::from_snapshot(&snapshot).unwrap();
    assert!(reopened.has_oversized_text());
    let ok = PageDoc::from_snapshot(&PageDoc::new().export_snapshot().unwrap()).unwrap();
    assert!(!ok.has_oversized_text());
}
