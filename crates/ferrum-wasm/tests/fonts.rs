//! Шрифты и текст в ядре worker'а (подсистема 3, этап 1): список вшитых,
//! запрос и загрузка байтов, правка, ждущая шрифтов, отсутствующие шрифты и
//! замена одним шагом, пределы команд, пример из 10 000 текстов.
#![cfg(not(target_arch = "wasm32"))]

use std::path::PathBuf;

use ferrum_editor::Platform;
use ferrum_render::CpuBackend;
use ferrum_text::{read_faces, MAX_FAMILY_NAME as MAX_FAMILY_NAME_BYTES};
use ferrum_wasm::core::Core;
use ferrum_wasm::protocol::*;
use kurbo::Size;
use serde_bytes::ByteBuf;

fn font_file(rel: &str) -> Vec<u8> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../apps/web/public/fonts")
        .join(rel);
    std::fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

const FILES: [&str; 4] = [
    "inter/Inter-Variable.ttf",
    "inter/Inter-Italic-Variable.ttf",
    "roboto/Roboto-Variable.ttf",
    "roboto/Roboto-Italic-Variable.ttf",
];

/// Манифест вшитых — как его отдаёт `font_manifest` (файлы относительно `fonts/`).
fn manifest() -> Vec<FontFamilyDto> {
    let faces = FILES
        .iter()
        .flat_map(|r| read_faces(&font_file(r), r))
        .collect::<Vec<_>>();
    ferrum_text::group(faces)
        .into_iter()
        .map(|f| FontFamilyDto {
            family: f.family,
            faces: f
                .faces
                .into_iter()
                .map(|x| FontFaceDto {
                    style: x.style,
                    weight: Some(x.weight as f64),
                    italic: Some(x.italic),
                    stretch: Some(x.stretch as f64),
                    file: x.file,
                    index: Some(x.index),
                })
                .collect(),
        })
        .collect()
}

fn core() -> Core {
    let mut c = Core::new(Platform::Mac, Size::new(400.0, 300.0), 1.0, Backend::Cpu);
    c.command(Command::Demo { nodes: 0 });
    c.command(Command::FontsAvailable {
        source: FontSource::Bundled,
        families: manifest(),
    });
    c.take_events();
    c
}

/// Отвечает на все запросы шрифтов из событий (как worker ядра по манифесту).
fn serve(c: &mut Core, events: &[CoreEvent]) -> usize {
    let mut n = 0;
    for e in events {
        if let CoreEvent::FontRequest {
            family,
            source,
            files,
        } = e
        {
            assert_eq!(*source, FontSource::Bundled);
            for f in files {
                c.load_font_for_tests(family, f, font_file(f));
                n += 1;
            }
        }
    }
    n
}

fn create(c: &mut Core, text: &str, font: Option<(&str, &str)>) {
    c.command(Command::TestCreateText {
        x: 20.0,
        y: 20.0,
        text: text.into(),
        font: font.map(|(f, s)| FontNameDto {
            family: f.into(),
            style: s.into(),
        }),
        size: Some(32.0),
        sizing: None,
        width: None,
        height: None,
    });
}

fn nodes(c: &Core) -> usize {
    c.editor().scene().len()
}

/// Кадр холста: число тёмных непрозрачных пикселей (тема светлая — холст светлый).
fn dark_pixels(c: &mut Core) -> usize {
    let (w, h) = c.device_size();
    let mut b = CpuBackend::new(w, h);
    c.draw(&mut b);
    let px = b.render();
    px.data().iter().filter(|p| p.r < 100 && p.a > 200).count()
}

#[test]
fn text_waits_for_requested_fonts_then_appears_and_draws() {
    let mut c = core();
    c.command(Command::SetTheme {
        theme: Theme::Light,
    });
    // Камера примера (зум 0.5, центр 720×512) показывает x 320…1120, y 212…812.
    let create_at = |c: &mut Core| {
        c.command(Command::TestCreateText {
            x: 600.0,
            y: 450.0,
            text: "Ferrum".into(),
            font: None,
            size: Some(64.0),
            sizing: None,
            width: None,
            height: None,
        })
    };
    create_at(&mut c);
    assert_eq!(nodes(&c), 0, "шрифт ещё не загружен");
    assert_eq!(dark_pixels(&mut c), 0, "до загрузки на холсте пусто");
    let events = c.take_events();
    assert_eq!(serve(&mut c, &events), 2);
    assert_eq!(nodes(&c), 1);
    let events = c.take_events();
    assert!(events
        .iter()
        .any(|e| matches!(e, CoreEvent::Document { can_undo: true, .. })));
    // Реестр сменился — кадр запрошен заново, и в нём глифы текста.
    let dark = dark_pixels(&mut c);
    assert!(dark > 100, "{dark}");
}

#[test]
fn missing_fonts_are_reported_and_replaced_in_one_step() {
    let mut c = core();
    create(&mut c, "Ferrum", None);
    let events = c.take_events();
    serve(&mut c, &events);
    create(&mut c, "Comic", Some(("Comic Sans", "Bold")));
    let events = c.take_events();
    let missing = events.iter().rev().find_map(|e| match e {
        CoreEvent::MissingFonts { list } => Some(list.clone()),
        _ => None,
    });
    assert_eq!(
        missing,
        Some(vec![MissingFontDto {
            family: "Comic Sans".into(),
            style: "Bold".into(),
            nodes: 1,
        }])
    );
    c.command(Command::ReplaceMissingFont {
        from: FontNameDto {
            family: "Comic Sans".into(),
            style: "Bold".into(),
        },
        to: FontNameDto {
            family: "Roboto".into(),
            style: "Bold".into(),
        },
    });
    // Roboto ещё не загружен: замена ждёт его, запросив байты.
    let events = c.take_events();
    assert!(events
        .iter()
        .any(|e| matches!(e, CoreEvent::FontRequest { family, .. } if family == "Roboto")));
    serve(&mut c, &events);
    let events = c.take_events();
    assert!(events
        .iter()
        .any(|e| matches!(e, CoreEvent::MissingFonts { list } if list.is_empty())));
    c.command(Command::Execute {
        command: "edit.undo".into(),
    });
    let events = c.take_events();
    assert!(events
        .iter()
        .any(|e| matches!(e, CoreEvent::MissingFonts { list } if list.len() == 1)));
}

#[test]
fn catalog_and_logs_and_limits() {
    let mut c = Core::new(Platform::Mac, Size::new(400.0, 300.0), 1.0, Backend::Cpu);
    c.command(Command::FontsAvailable {
        source: FontSource::Bundled,
        families: manifest(),
    });
    let events = c.take_events();
    let catalog = events.iter().find_map(|e| match e {
        CoreEvent::FontCatalog { families } => Some(families.clone()),
        _ => None,
    });
    let catalog = catalog.expect("каталог");
    assert!(catalog
        .iter()
        .any(|f| f.family == "Inter" && f.styles.contains(&"Bold".to_string())));
    assert!(catalog.iter().any(|f| f.family == "Onest"));
    // Неразборчивые байты — сообщение в журнал, не уведомление.
    c.command(Command::LoadFont {
        family: "Inter".into(),
        file: FILES[0].into(),
        bytes: Some(ByteBuf::from(b"garbage".to_vec())),
    });
    let events = c.take_events();
    assert!(events.iter().any(|e| matches!(e, CoreEvent::Log { .. })));
    assert!(!events.iter().any(|e| matches!(e, CoreEvent::Error { .. })));
    // Пределы: длинное имя семейства, длинный текст.
    c.command(Command::FontsAvailable {
        source: FontSource::System,
        families: vec![FontFamilyDto {
            family: "x".repeat(300),
            faces: vec![],
        }],
    });
    c.command(Command::TestCreateText {
        x: 0.0,
        y: 0.0,
        text: "я".repeat(40_000),
        font: None,
        size: None,
        sizing: None,
        width: None,
        height: None,
    });
    let events = c.take_events();
    // Неверное семейство пропущено с записью в журнал; длинный текст — отказ.
    assert_eq!(errors_of(&events).len(), 1);
    assert!(events.iter().any(|e| matches!(e, CoreEvent::Log { .. })));
}

#[test]
fn demo_with_ten_thousand_texts_opens_without_layout() {
    let mut c = core();
    let t = std::time::Instant::now();
    c.command(Command::DemoText { texts: 10_000 });
    let elapsed = t.elapsed();
    let events = c.take_events();
    let opened = events.iter().find_map(|e| match e {
        CoreEvent::Opened { nodes, .. } => Some(*nodes),
        _ => None,
    });
    assert_eq!(opened, Some(10_000 + 100));
    // Шрифты страницы запрошены сразу (а не при первом рисовании), и один раз.
    let requests = events
        .iter()
        .filter(|e| matches!(e, CoreEvent::FontRequest { family, .. } if family == "Inter"))
        .count();
    assert_eq!(requests, 1);
    eprintln!("пример из 10 000 текстов: {elapsed:?}");
}

/// Неверные семейства и начертания пропускаются с записью в журнал, лишнее сверх
/// пределов отбрасывается, остальной список принимается (а не весь отвергается).
#[test]
fn font_list_skips_bad_entries_and_keeps_the_rest() {
    let mut c = Core::new(Platform::Mac, Size::new(400.0, 300.0), 1.0, Backend::Cpu);
    let face = |style: &str, file: String| FontFaceDto {
        style: style.into(),
        weight: None,
        italic: None,
        stretch: None,
        file,
        index: None,
    };
    let family = |name: &str, faces| FontFamilyDto {
        family: name.into(),
        faces,
    };
    let mut list = vec![
        // Длинный путь у одного начертания — пропущено только оно.
        family(
            "Mixed",
            vec![
                face("Regular", "f".repeat(MAX_TEXT_BYTES + 1)),
                face("Bold", "f".repeat(MAX_TEXT_BYTES)),
                face("", "e".into()),
            ],
        ),
        // Начертаний больше 1000 — берутся первые 1000.
        family(
            "Many",
            (0..=MAX_FONT_FACES)
                .map(|i| face(&format!("S{i}"), format!("m{i}")))
                .collect(),
        ),
        // Неверное имя семейства — пропущено семейство.
        family(
            &"x".repeat(MAX_FAMILY_NAME_BYTES + 1),
            vec![face("Regular", "x".into())],
        ),
    ];
    // Семейств больше 4096 — берутся первые.
    list.extend(
        (0..MAX_FONT_FAMILIES)
            .map(|i| family(&format!("F{i}"), vec![face("Regular", format!("g{i}"))])),
    );
    c.command(Command::FontsAvailable {
        source: FontSource::System,
        families: list,
    });
    let events = c.take_events();
    assert!(errors_of(&events).is_empty(), "{:?}", errors_of(&events));
    let logs = events
        .iter()
        .filter(|e| matches!(e, CoreEvent::Log { .. }))
        .count();
    assert!(
        logs >= 4,
        "журнал: семейства, начертания, предел начертаний, предел семейств"
    );
    let catalog = events
        .iter()
        .rev()
        .find_map(|e| match e {
            CoreEvent::FontCatalog { families } => Some(families.clone()),
            _ => None,
        })
        .expect("каталог");
    let styles = |name: &str| {
        catalog
            .iter()
            .find(|f| f.family == name)
            .map(|f| f.styles.clone())
            .unwrap_or_default()
    };
    assert_eq!(styles("Mixed"), vec!["Bold".to_string()]);
    assert_eq!(styles("Many").len(), MAX_FONT_FACES);
    assert!(styles("F0").len() == 1);
    // Последнее семейство — за пределом списка.
    let last = format!("F{}", MAX_FONT_FAMILIES - 1);
    assert!(styles(&last).is_empty());
}

#[test]
fn document_refusal_is_reported_and_not_queued() {
    // Файл «из будущего» открывается только для чтения: правка текста — отказ
    // документа, а не ожидание шрифтов.
    let bytes = ferrum_doc::container::encode(&ferrum_doc::Document::new()).unwrap();
    let future =
        ferrum_doc::container::testing::with_schema_version(&bytes, ferrum_doc::SCHEMA_VERSION + 1);
    let mut c = core();
    c.command(Command::Open {
        bytes: ByteBuf::from(future),
    });
    c.take_events();
    // Inter нужен для раскладки, чтобы правка дошла до документа.
    create(&mut c, "Ferrum", None);
    let events = c.take_events();
    serve(&mut c, &events);
    let events = c.take_events();
    let messages: Vec<&String> = events
        .iter()
        .filter_map(|e| match e {
            CoreEvent::Error { message } => Some(message),
            _ => None,
        })
        .collect();
    assert_eq!(messages.len(), 1, "{events:?}");
    assert!(messages[0].contains("только для чтения"), "{messages:?}");
    assert_eq!(nodes(&c), 0);
    // Очередь пуста: следующее изменение реестра не повторяет правку.
    c.load_font_for_tests("Inter", FILES[0], font_file(FILES[0]));
    assert!(!c
        .take_events()
        .iter()
        .any(|e| matches!(e, CoreEvent::Error { .. })));
}

fn errors_of(events: &[CoreEvent]) -> Vec<String> {
    events
        .iter()
        .filter_map(|e| match e {
            CoreEvent::Error { message } => Some(message.clone()),
            _ => None,
        })
        .collect()
}

const STILL_LOADING: &str = "Шрифты ещё загружаются — повторите через мгновение";

#[test]
fn export_copy_and_thumbnail_do_not_leave_loading_text_out() {
    let mut c = core();
    c.command(Command::DemoText { texts: 100 });
    let loading = c.take_events();
    c.command(Command::Export { scale: 1.0 });
    let ev = c.take_events();
    assert_eq!(errors_of(&ev), [STILL_LOADING]);
    assert!(!ev.iter().any(|e| matches!(e, CoreEvent::Exported { .. })));
    c.command(Command::Execute {
        command: "edit.select_all".into(),
    });
    c.command(Command::Execute {
        command: "edit.copy_png".into(),
    });
    let ev = c.take_events();
    assert_eq!(errors_of(&ev), [STILL_LOADING]);
    assert!(!ev
        .iter()
        .any(|e| matches!(e, CoreEvent::ClipboardPng { .. })));
    c.command(Command::Execute {
        command: "edit.copy_svg".into(),
    });
    let ev = c.take_events();
    assert_eq!(errors_of(&ev), [STILL_LOADING]);
    assert!(!ev
        .iter()
        .any(|e| matches!(e, CoreEvent::ClipboardSvg { .. })));
    // Превью: прежнее остаётся.
    c.command(Command::Snapshot { thumbnail: true });
    let ev = c.take_events();
    assert!(ev.iter().any(|e| matches!(
        e,
        CoreEvent::Snapshot {
            thumbnail: None,
            keep_thumbnail: true,
            ..
        }
    )));
    // Шрифты пришли — всё работает.
    serve(&mut c, &loading);
    c.take_events();
    c.command(Command::Export { scale: 1.0 });
    let ev = c.take_events();
    assert!(errors_of(&ev).is_empty(), "{ev:?}");
    assert!(ev.iter().any(|e| matches!(e, CoreEvent::Exported { .. })));
    c.command(Command::Snapshot { thumbnail: true });
    let ev = c.take_events();
    assert!(ev.iter().any(|e| matches!(
        e,
        CoreEvent::Snapshot {
            thumbnail: Some(_),
            keep_thumbnail: false,
            ..
        }
    )));
}

#[test]
fn export_contains_text_pixels_once_fonts_are_loaded() {
    let mut c = core();
    create(&mut c, "Ferrum", None);
    let events = c.take_events();
    serve(&mut c, &events);
    c.take_events();
    c.command(Command::Export { scale: 1.0 });
    let ev = c.take_events();
    let bytes = ev
        .iter()
        .find_map(|e| match e {
            CoreEvent::Exported { bytes, .. } => Some(bytes.clone()),
            _ => None,
        })
        .unwrap_or_else(|| panic!("{ev:?}"));
    let img = ferrum_render::decode_png(&bytes).unwrap();
    // Кроме текста, на странице ничего нет: непрозрачные пиксели — глифы.
    let ink = img.data().iter().filter(|p| p.a > 128).count();
    assert!(ink > 50, "{ink}");
}

#[test]
fn opening_a_document_drops_edits_waiting_for_fonts() {
    let mut c = core();
    create(&mut c, "Ferrum", None);
    let events = c.take_events();
    c.command(Command::Demo { nodes: 0 });
    c.take_events();
    serve(&mut c, &events);
    assert_eq!(nodes(&c), 0, "правка не должна попасть в другой документ");
}

#[test]
fn page_change_waits_behind_font_replacement() {
    let mut c = core();
    // Inter готов; на двух страницах по тексту шрифтом «Comic Sans».
    create(&mut c, "Ferrum", None);
    let events = c.take_events();
    serve(&mut c, &events);
    let comic = Some(("Comic Sans", "Bold"));
    create(&mut c, "Один", comic);
    let first = c.editor().document().pages()[0].0.to_string();
    c.command(Command::AddPage);
    create(&mut c, "Два", comic);
    c.command(Command::SetPage { id: first.clone() });
    c.command(Command::ReplaceMissingFont {
        from: FontNameDto {
            family: "Comic Sans".into(),
            style: "Bold".into(),
        },
        to: FontNameDto {
            family: "Roboto".into(),
            style: "Bold".into(),
        },
    });
    let events = c.take_events();
    // Замена ждёт Roboto; смена страницы встаёт за ней: замена — на первой
    // странице, где её попросили, а не на второй.
    let second = c.editor().document().pages()[1].0.to_string();
    c.command(Command::SetPage { id: second.clone() });
    c.take_events();
    assert_eq!(c.editor().page().to_string(), first, "смена страницы ждёт");
    serve(&mut c, &events);
    assert_eq!(c.editor().page().to_string(), second);
    let missing = c.editor().missing_fonts();
    assert_eq!(missing.len(), 1, "{missing:?}");
    assert_eq!(missing[0].font.family, "Comic Sans");
    c.command(Command::SetPage { id: first });
    assert!(
        c.editor().missing_fonts().is_empty(),
        "первая страница — Roboto"
    );
}

#[test]
fn failed_load_releases_waiting_edit_with_fallback() {
    let mut c = core();
    create(&mut c, "Ferrum", None);
    let events = c.take_events();
    for e in &events {
        if let CoreEvent::FontRequest { family, files, .. } = e {
            for f in files {
                c.command(Command::LoadFont {
                    family: family.clone(),
                    file: f.clone(),
                    bytes: None,
                });
            }
        }
    }
    assert_eq!(nodes(&c), 1);
    let events = c.take_events();
    assert!(events.iter().any(|e| matches!(e, CoreEvent::Log { .. })));
    assert!(errors_of(&events).is_empty());
}

fn font_lists_sent(events: &[CoreEvent]) -> (usize, usize) {
    let missing = events
        .iter()
        .filter(|e| matches!(e, CoreEvent::MissingFonts { .. }))
        .count();
    let catalog = events
        .iter()
        .filter(|e| matches!(e, CoreEvent::FontCatalog { .. }))
        .count();
    (missing, catalog)
}

/// Отсутствующие шрифты и каталог не пересчитываются от правок без текста
/// (сдвиг стрелками, отмена сдвига): только от смены шрифта текста, смены
/// страницы и изменения реестра.
#[test]
fn nudge_does_not_rescan_missing_fonts_or_catalog() {
    let mut c = core();
    create(&mut c, "Ferrum", None);
    let events = c.take_events();
    serve(&mut c, &events);
    create(&mut c, "Comic", Some(("Comic Sans", "Bold")));
    c.command(Command::Execute {
        command: "edit.select_all".into(),
    });
    c.take_events();
    let before = c.font_recomputes_for_tests();
    for _ in 0..10 {
        c.command(Command::Execute {
            command: "nudge.right".into(),
        });
    }
    c.command(Command::Execute {
        command: "edit.undo".into(),
    });
    let events = c.take_events();
    assert!(events
        .iter()
        .any(|e| matches!(e, CoreEvent::Document { .. })));
    assert_eq!(
        c.font_recomputes_for_tests(),
        before,
        "сдвиг — без пересчёта"
    );
    assert_eq!(font_lists_sent(&events), (0, 0), "и без повторных списков");

    // Смена шрифта текста: отсутствующие — заново (каталог не менялся).
    c.command(Command::ReplaceMissingFont {
        from: FontNameDto {
            family: "Comic Sans".into(),
            style: "Bold".into(),
        },
        to: FontNameDto {
            family: "Inter".into(),
            style: "Bold".into(),
        },
    });
    let events = c.take_events();
    let after_replace = c.font_recomputes_for_tests();
    assert!(after_replace.0 > before.0);
    assert!(events
        .iter()
        .any(|e| matches!(e, CoreEvent::MissingFonts { list } if list.is_empty())));

    // Смена страницы: отсутствующие — заново.
    c.command(Command::AddPage);
    c.take_events();
    let after_page = c.font_recomputes_for_tests();
    assert!(after_page.0 > after_replace.0);

    // Загрузка шрифта меняет реестр: и отсутствующие, и каталог.
    c.load_font_for_tests("Roboto", FILES[2], font_file(FILES[2]));
    c.take_events();
    let after_load = c.font_recomputes_for_tests();
    assert!(after_load.0 > after_page.0 && after_load.1 > after_page.1);
}
