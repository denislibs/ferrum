//! Текстовый узел (подсистема 3, этап 1): содержимое в `LoroText` с разметкой,
//! расширение разметки на вставку, свойства текста в зеркале, отмена правок
//! текста обратной дельтой, слияние ввода подряд, пределы, файл и кэш сцены.

use ferrum_doc::container::{decode, encode};
use ferrum_doc::*;

/// Правка текста внутри транзакции (для таблиц правок в тестах).
type Edit = Box<dyn Fn(&Tx) -> DocResult<()>>;

fn bold() -> TextAttrs {
    TextAttrs {
        style: Some("Bold".into()),
        ..Default::default()
    }
}

/// Документ с одним текстовым узлом «Привет, мир» (без разметки).
fn doc_with_text() -> (Document, PageId, NodeId) {
    let mut doc = Document::new();
    let page = doc.pages()[0].0;
    let id = doc
        .transact(page, vec![], |tx| {
            let id = tx.create_node(None, None, NodeKind::Text)?;
            tx.set_text_content(id, &TextContent::plain("Привет, мир"))?;
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

fn texts(c: &TextContent) -> Vec<(&str, &TextAttrs)> {
    c.runs.iter().map(|r| (r.text.as_str(), &r.attrs)).collect()
}

#[test]
fn new_text_node_has_figma_defaults() {
    let (mut doc, page, id) = doc_with_text();
    let p = doc.page(page).unwrap().props(id).unwrap();
    assert_eq!(p.kind, NodeKind::Text);
    assert_eq!(p.name, "Текст");
    assert_eq!(p.fills, vec![Fill::solid(Color::BLACK)]);
    let t = p.text.unwrap();
    assert_eq!(t.props, TextProps::default());
    assert_eq!(t.props.style.font, FontName::new("Inter", "Regular"));
    assert_eq!(t.props.style.size, 12.0);
    assert_eq!(t.content.text(), "Привет, мир\n");
    assert_eq!(t.content.char_len(), 12);
}

#[test]
fn marks_expand_to_the_right_and_paragraph_marks_do_not() {
    let (mut doc, page, id) = doc_with_text();
    doc.transact(page, vec![], |tx| {
        // «Привет» — жирным; вставка сразу за ним наследует жирный (как в редакторах).
        tx.format_text(id, 0..6, &bold(), &[])?;
        tx.insert_text(id, 6, "!", None)?;
        // Список — на переводе строки абзаца; вставка перед ним список не получает.
        let list = TextAttrs {
            list: Some(ListKind::Bullet),
            indent: Some(1),
            ..Default::default()
        };
        tx.format_text(id, 12..13, &list, &[])?;
        tx.insert_text(id, 12, "?", None)?;
        Ok(vec![])
    })
    .unwrap();
    let c = content(&mut doc, page, id);
    assert_eq!(c.text(), "Привет!, мир?\n");
    let runs = texts(&c);
    assert_eq!(runs[0], ("Привет!", &bold()));
    assert_eq!(runs[1].0, ", мир?");
    assert!(runs[1].1.is_empty());
    assert_eq!(runs[2].0, "\n");
    assert_eq!(runs[2].1.list, Some(ListKind::Bullet));
    assert_eq!(runs[2].1.indent, Some(1));
}

#[test]
fn explicit_insert_attrs_replace_inherited_ones() {
    let (mut doc, page, id) = doc_with_text();
    let red = TextAttrs {
        fill: Some(Fill::solid(Color::rgba(255, 0, 0, 255))),
        ..Default::default()
    };
    doc.transact(page, vec![], |tx| {
        tx.format_text(id, 0..6, &bold(), &[])?;
        // Явная разметка вставки — ровно она, без унаследованного жирного.
        tx.insert_text(id, 6, "X", Some(&red))?;
        Ok(vec![])
    })
    .unwrap();
    let c = content(&mut doc, page, id);
    assert_eq!(texts(&c)[1], ("X", &red));
}

#[test]
fn every_text_edit_undoes_and_redoes_exactly() {
    let (mut doc, page, id) = doc_with_text();
    let start = content(&mut doc, page, id);
    let size = TextAttrs {
        size: Some(24.0),
        decoration: Some(Decoration::Underline),
        ..Default::default()
    };
    let edits: Vec<Edit> = vec![
        Box::new(move |tx| tx.format_text(id, 0..6, &bold(), &[])),
        Box::new(move |tx| tx.format_text(id, 2..9, &size, &[])),
        Box::new(move |tx| tx.insert_text(id, 3, "ёж", None)),
        Box::new(move |tx| tx.delete_text(id, 1..8)),
        Box::new(move |tx| tx.format_text(id, 0..4, &TextAttrs::default(), &[AttrKey::Style])),
        Box::new(move |tx| tx.set_text_content(id, &TextContent::plain("Новый\nтекст"))),
    ];
    let mut states = vec![start];
    for edit in &edits {
        doc.transact(page, vec![id], |tx| {
            edit(tx)?;
            Ok(vec![id])
        })
        .unwrap();
        states.push(content(&mut doc, page, id));
    }
    assert_eq!(states.last().unwrap().text(), "Новый\nтекст\n");
    for i in (0..edits.len()).rev() {
        doc.undo().unwrap();
        assert_eq!(content(&mut doc, page, id), states[i], "отмена шага {i}");
    }
    for (i, s) in states.iter().enumerate().skip(1) {
        doc.redo().unwrap();
        assert_eq!(&content(&mut doc, page, id), s, "повтор шага {i}");
    }
}

#[test]
fn text_survives_delete_and_undo_and_old_text_steps_still_undo() {
    let (mut doc, page, id) = doc_with_text();
    doc.transact(page, vec![], |tx| {
        tx.format_text(id, 0..6, &bold(), &[])?;
        Ok(vec![])
    })
    .unwrap();
    let styled = content(&mut doc, page, id);
    doc.transact(page, vec![], |tx| {
        tx.delete_nodes(&[id])?;
        Ok(vec![])
    })
    .unwrap();
    assert!(!doc.page(page).unwrap().contains(id));
    // Восстановленный узел — с тем же текстом и разметкой (новый `LoroText`).
    doc.undo().unwrap();
    assert_eq!(content(&mut doc, page, id), styled);
    // Отмена правки текста, записанной до удаления, — уже на новом контейнере.
    doc.undo().unwrap();
    let plain = content(&mut doc, page, id);
    assert_eq!(plain.runs.len(), 1);
    assert_eq!(plain.text(), "Привет, мир\n");
}

#[test]
fn text_props_patch_writes_only_text_nodes() {
    let (mut doc, page, id) = doc_with_text();
    let props = TextProps {
        sizing: TextSizing::Fixed,
        align_h: AlignH::Center,
        max_lines: Some(2),
        truncate: true,
        style: TextStyle {
            font: FontName::new("Roboto", "Semi Bold Italic"),
            line_height: LineHeight::Percent(150.0),
            letter_spacing: LetterSpacing::Px(-0.5),
            ..TextStyle::default()
        },
        ..TextProps::default()
    };
    let rect = doc
        .transact(page, vec![], |tx| {
            tx.set_props(
                id,
                &PropsPatch {
                    text: Some(props.clone()),
                    ..Default::default()
                },
            )?;
            let r = tx.create_node(None, None, NodeKind::Rect)?;
            // Патч со свойствами текста у прямоугольника их не создаёт.
            tx.set_props(
                r,
                &PropsPatch {
                    text: Some(props.clone()),
                    x: Some(5.0),
                    ..Default::default()
                },
            )?;
            Ok(vec![r])
        })
        .unwrap()[0];
    let p = doc.page(page).unwrap();
    assert_eq!(p.props(id).unwrap().text_props(), Some(&props));
    assert_eq!(p.props(rect).unwrap().text, None);
    assert_eq!(p.props(rect).unwrap().x, 5.0);
    // Зеркало совпадает с честным чтением из Loro.
    let reloaded = PageDoc::from_snapshot(&p.export_snapshot().unwrap()).unwrap();
    assert_eq!(reloaded.props(id).unwrap(), p.props(id).unwrap());
    assert_eq!(reloaded.props(rect).unwrap().text, None);
    doc.undo().unwrap();
    let p = doc.page(page).unwrap();
    assert_eq!(
        p.props(id).unwrap().text_props(),
        Some(&TextProps::default())
    );
}

#[test]
fn typing_in_a_row_is_one_undo_step() {
    let (mut doc, page, id) = doc_with_text();
    let before = content(&mut doc, page, id);
    // Курсор с 11-го символа, правки через 300 мс.
    for (i, ch) in ["!", "!", "?"].into_iter().enumerate() {
        let caret = 11 + i;
        let merge = TextMerge {
            node: id,
            before: caret,
            after: caret + 1,
            style: 0,
            at_ms: 300.0 * i as f64,
        };
        doc.transact_merging(page, vec![id], merge, |tx| {
            tx.insert_text(id, caret, ch, None)?;
            Ok(vec![id])
        })
        .unwrap();
    }
    assert_eq!(content(&mut doc, page, id).text(), "Привет, мир!!?\n");
    doc.undo().unwrap();
    assert_eq!(content(&mut doc, page, id), before);
    assert!(
        !doc.can_undo() || {
            // Создание узла — свой шаг раньше ввода.
            doc.undo().unwrap();
            !doc.can_undo()
        }
    );
}

#[test]
fn pause_jump_or_style_change_starts_a_new_step() {
    for (gap, jump, style) in [(1500.0, 0, 0), (100.0, 3, 0), (100.0, 0, 7)] {
        let (mut doc, page, id) = doc_with_text();
        let first = TextMerge {
            node: id,
            before: 0,
            after: 1,
            style: 0,
            at_ms: 0.0,
        };
        doc.transact_merging(page, vec![], first, |tx| {
            tx.insert_text(id, 0, "а", None)?;
            Ok(vec![])
        })
        .unwrap();
        let after_first = content(&mut doc, page, id);
        let at = 1 + jump;
        let second = TextMerge {
            node: id,
            before: at,
            after: at + 1,
            style,
            at_ms: gap,
        };
        doc.transact_merging(page, vec![], second, |tx| {
            tx.insert_text(id, at, "б", None)?;
            Ok(vec![])
        })
        .unwrap();
        doc.undo().unwrap();
        assert_eq!(
            content(&mut doc, page, id),
            after_first,
            "{gap} {jump} {style}"
        );
    }
}

#[test]
fn undo_breaks_merging() {
    let (mut doc, page, id) = doc_with_text();
    let tag = |before: usize, at_ms: f64| TextMerge {
        node: id,
        before,
        after: before + 1,
        style: 0,
        at_ms,
    };
    doc.transact_merging(page, vec![], tag(0, 0.0), |tx| {
        tx.insert_text(id, 0, "а", None)?;
        Ok(vec![])
    })
    .unwrap();
    doc.undo().unwrap();
    let base = content(&mut doc, page, id);
    // После отмены та же позиция не сливается с отменённым шагом.
    doc.transact_merging(page, vec![], tag(1, 100.0), |tx| {
        tx.insert_text(id, 0, "б", None)?;
        Ok(vec![])
    })
    .unwrap();
    doc.undo().unwrap();
    assert_eq!(content(&mut doc, page, id), base);
}

#[test]
fn limits_refuse_the_edit_without_a_step() {
    let (mut doc, page, id) = doc_with_text();
    let before = content(&mut doc, page, id);
    let long = "я".repeat(MAX_TEXT_CHARS);
    let err = doc.transact(page, vec![], |tx| {
        tx.insert_text(id, 0, &long, None)?;
        Ok(vec![])
    });
    assert!(matches!(err, Err(DocError::TextTooLong)), "{err:?}");
    assert_eq!(content(&mut doc, page, id), before);
    // 10 001 участок со стилем одной правкой: буквы попеременно 10 и 11 px.
    let runs: Vec<TextRun> = (0..=MAX_TEXT_RUNS)
        .map(|i| TextRun {
            text: "a".into(),
            attrs: TextAttrs {
                size: Some(10.0 + (i % 2) as f64),
                ..Default::default()
            },
        })
        .collect();
    let ok = content(&mut doc, page, id);
    let err = doc.transact(page, vec![], |tx| {
        tx.set_text_content(id, &TextContent { runs })?;
        Ok(vec![])
    });
    assert!(matches!(err, Err(DocError::TooManyTextRuns)), "{err:?}");
    assert_eq!(content(&mut doc, page, id), ok);
    // Отказы не добавили шагов: единственный шаг отмены — создание узла.
    doc.undo().unwrap();
    assert!(!doc.page(page).unwrap().contains(id));
}

#[test]
fn positions_outside_the_text_are_refused() {
    let (mut doc, page, id) = doc_with_text();
    for bad in [
        Box::new(move |tx: &Tx| tx.insert_text(id, 12, "x", None)) as Edit,
        Box::new(move |tx: &Tx| tx.delete_text(id, 5..12)),
        Box::new(move |tx: &Tx| tx.format_text(id, 3..13, &bold(), &[])),
    ] {
        let err = doc.transact(page, vec![], |tx| {
            bad(tx)?;
            Ok(vec![])
        });
        assert!(matches!(err, Err(DocError::TextRange(..))), "{err:?}");
    }
    let rect = doc
        .transact(page, vec![], |tx| {
            Ok(vec![tx.create_node(None, None, NodeKind::Rect)?])
        })
        .unwrap()[0];
    let err = doc.transact(page, vec![], |tx| {
        tx.insert_text(rect, 0, "x", None)?;
        Ok(vec![])
    });
    assert!(matches!(err, Err(DocError::NotText(_))), "{err:?}");
}

#[test]
fn text_survives_file_roundtrip_with_and_without_scene_cache() {
    let (mut doc, page, id) = doc_with_text();
    let red = Fill::solid(Color::rgba(200, 10, 10, 255));
    doc.transact(page, vec![], |tx| {
        tx.format_text(
            id,
            0..6,
            &TextAttrs {
                font: Some("Roboto".into()),
                style: Some("Bold Italic".into()),
                size: Some(20.0),
                fill: Some(red.clone()),
                line_height: Some(LineHeight::Px(30.0)),
                letter_spacing: Some(LetterSpacing::Percent(5.0)),
                decoration: Some(Decoration::Strike),
                case: Some(TextCase::Upper),
                ..Default::default()
            },
            &[],
        )?;
        tx.format_text(
            id,
            11..12,
            &TextAttrs {
                list: Some(ListKind::Ordered),
                indent: Some(2),
                ..Default::default()
            },
            &[],
        )?;
        tx.set_props(
            id,
            &PropsPatch {
                w: Some(120.0),
                h: Some(40.0),
                ..Default::default()
            },
        )?;
        Ok(vec![])
    })
    .unwrap();
    let props = doc.page(page).unwrap().props(id).unwrap();
    let bytes = encode(&doc).unwrap();
    // С кэшем сцены: зеркало из таблицы, без чтения Loro.
    let mut cached = decode(&bytes).unwrap().document;
    assert!(cached.page(page).unwrap().loaded_from_cache());
    assert_eq!(cached.page(page).unwrap().props(id).unwrap(), props);
    // Без кэша (секция с чужим id): зеркало из Loro — то же самое.
    let no_cache = container::testing::map_sections(&bytes, |kind, sid, _| {
        if kind == 5 {
            *sid = "не-страница".into();
        }
    });
    let mut fresh = decode(&no_cache).unwrap().document;
    assert!(!fresh.page(page).unwrap().loaded_from_cache());
    assert_eq!(fresh.page(page).unwrap().props(id).unwrap(), props);
    // Открытый файл правится: расширение разметки задано и у загруженной страницы.
    fresh
        .transact(page, vec![], |tx| {
            tx.insert_text(id, 6, "!", None)?;
            Ok(vec![])
        })
        .unwrap();
    let c = content(&mut fresh, page, id);
    assert_eq!(c.runs[0].text, "Привет!");
    assert_eq!(c.runs[0].attrs.fill, Some(red));
}

#[test]
fn text_edits_go_through_the_journal() {
    let (mut doc, page, id) = doc_with_text();
    let file = encode(&doc).unwrap();
    doc.mark_journal();
    doc.transact(page, vec![], |tx| {
        tx.insert_text(id, 0, "Ну, ", None)?;
        tx.format_text(id, 0..3, &bold(), &[])?;
        Ok(vec![])
    })
    .unwrap();
    let batch = doc.take_journal().unwrap().unwrap();
    let mut restored = decode(&file).unwrap().document;
    let replayed = restored.apply_journal(&[&batch]);
    assert_eq!(replayed.applied, 1, "{:?}", replayed.warnings);
    assert_eq!(
        content(&mut restored, page, id),
        content(&mut doc, page, id)
    );
}

/// `n` участков со стилями, между ними обычные; кончается переводом строки.
fn styled(n: usize) -> TextContent {
    let mut runs: Vec<TextRun> = (0..n)
        .flat_map(|i| {
            [
                TextRun {
                    text: "а".into(),
                    attrs: TextAttrs {
                        size: Some(10.0 + i as f64),
                        ..Default::default()
                    },
                },
                TextRun {
                    text: "б".into(),
                    attrs: TextAttrs::default(),
                },
            ]
        })
        .collect();
    if let Some(last) = runs.last_mut() {
        last.text.push('\n');
    }
    TextContent { runs }
}

#[test]
fn text_json_round_trip() {
    let mut props = TextProps::default();
    props.style.size = 20.0;
    let data = TextData {
        props,
        content: {
            let mut c = styled(2);
            c.runs[0].attrs.style = Some("Bold".into());
            c
        },
    };
    let back = TextData::from_json(&data.to_json()).unwrap();
    assert_eq!(back, data);
}

#[test]
fn text_json_bad_input() {
    assert!(TextData::from_json("не json").is_err());
    assert!(TextData::from_json("{}").is_err());
    assert!(TextData::from_json(r#"{"props":{}}"#).is_err());
    assert!(TextData::from_json(r#"{"runs":[]}"#).is_err());
    assert!(TextData::from_json(r#"{"props":{},"runs":[{"attributes":{}}]}"#).is_err());
    // Неизвестные ключи свойств — умолчания.
    let d = TextData::from_json(r#"{"props":{"zzz":1},"runs":[{"insert":"a"}]}"#).unwrap();
    assert_eq!(d.props, TextProps::default());
    assert_eq!(d.content.text(), "a\n");
}

#[test]
fn text_json_adds_trailing_newline_and_refuses_over_limit() {
    let d = TextData::from_json(r#"{"props":{},"runs":[{"insert":"абв"}]}"#).unwrap();
    assert_eq!(d.content.text(), "абв\n");
    let d = TextData::from_json(r#"{"props":{},"runs":[]}"#).unwrap();
    assert_eq!(d.content.text(), "\n");

    let chars = |n: usize| {
        format!(
            r#"{{"props":{{}},"runs":[{{"insert":"{}"}}]}}"#,
            "a".repeat(n)
        )
    };
    assert!(TextData::from_json(&chars(MAX_TEXT_CHARS)).is_ok());
    assert!(matches!(
        TextData::from_json(&chars(MAX_TEXT_CHARS + 1)),
        Err(DocError::TextTooLong)
    ));

    let runs = |n: usize| {
        let items: Vec<String> = (0..n)
            .map(|i| {
                format!(
                    r#"{{"insert":"а","attributes":{{"size":{}}}}},{{"insert":"б"}}"#,
                    10 + i
                )
            })
            .collect();
        format!(r#"{{"props":{{}},"runs":[{}]}}"#, items.join(","))
    };
    assert!(TextData::from_json(&runs(MAX_TEXT_RUNS)).is_ok());
    assert!(matches!(
        TextData::from_json(&runs(MAX_TEXT_RUNS + 1)),
        Err(DocError::TooManyTextRuns)
    ));
}

#[test]
fn create_with_props_undo_redo() {
    let (mut doc, page, id) = doc_with_text();
    let mut props = doc.page(page).unwrap().props(id).unwrap();
    props.name = "Копия".into();
    let copy = doc
        .transact(page, vec![], |tx| {
            Ok(vec![tx.create_with_props(None, None, &props)?])
        })
        .unwrap()[0];
    let want = props.text.clone();
    let got = |doc: &mut Document| doc.page(page).unwrap().props(copy);
    assert_eq!(got(&mut doc).unwrap().text, want);
    assert_eq!(got(&mut doc).unwrap().name, "Копия");
    doc.undo().unwrap();
    assert!(got(&mut doc).is_err());
    doc.redo().unwrap();
    assert_eq!(got(&mut doc).unwrap().text, want);
}

#[test]
fn create_with_props_refuses_over_limit_without_step() {
    let (mut doc, page, id) = doc_with_text();
    let base = doc.page(page).unwrap().props(id).unwrap();
    for content in [
        TextContent::plain(&"a".repeat(MAX_TEXT_CHARS + 1)),
        styled(MAX_TEXT_RUNS + 1),
    ] {
        let mut props = base.clone();
        props.text = Some(std::sync::Arc::new(TextData {
            props: TextProps::default(),
            content,
        }));
        let nodes = doc.page(page).unwrap().walk().len();
        let r = doc.transact(page, vec![], |tx| {
            Ok(vec![tx.create_with_props(None, None, &props)?])
        });
        assert!(matches!(
            r,
            Err(DocError::TextTooLong | DocError::TooManyTextRuns)
        ));
        assert_eq!(doc.page(page).unwrap().walk().len(), nodes);
    }
    // Отменяется только создание исходного узла: отказанные копии шагов не дали.
    doc.undo().unwrap();
    assert!(!doc.can_undo());
}

#[test]
fn fill_texts_counts() {
    let mut doc = Document::new();
    let page = doc.pages()[0].0;
    let ids = doc
        .transact(page, vec![], |tx| fixture::fill_texts(tx, 250))
        .unwrap();
    assert_eq!(ids.len(), 250);
    let p = doc.page(page).unwrap();
    let walk = p.walk();
    let count = |k| walk.iter().filter(|n| p.kind(**n).unwrap() == k).count();
    assert_eq!((count(NodeKind::Text), count(NodeKind::Frame)), (250, 3));
}
