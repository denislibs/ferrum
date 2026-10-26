//! Хранилище десктопа (этап 6): новый файл и запись, атомарная запись без временных
//! файлов и без порчи при отказе, чужая запись (поколение) и чужая программа (время
//! изменения), журнал по порядку и его стирание, разделы и поиск, удаление своих и
//! чужих файлов, переезд на диск, испорченный список, настройки.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use ferrum_desktop_store::*;

static NEXT: AtomicU64 = AtomicU64::new(0);

thread_local! {
    static LAST: std::cell::RefCell<Option<PathBuf>> = const { std::cell::RefCell::new(None) };
}

/// Убирает каталог теста при выходе из теста (в том числе при панике).
struct Cleanup(Option<PathBuf>);

impl Drop for Cleanup {
    fn drop(&mut self) {
        if let Some(p) = self.0.take() {
            // Каталог мог остаться только для чтения после проверки отказа записи.
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                for sub in ["data", "docs"] {
                    let _ = fs::set_permissions(p.join(sub), fs::Permissions::from_mode(0o755));
                }
            }
            let _ = fs::remove_dir_all(p);
        }
    }
}

/// Охранник каталога, созданного последним `dirs()` этого теста.
fn cleanup() -> Cleanup {
    Cleanup(LAST.with(|l| l.borrow_mut().take()))
}

/// Свой пустой каталог на тест: данные приложения и документы.
fn dirs() -> (PathBuf, PathBuf) {
    let base = std::env::temp_dir().join(format!(
        "ferrum-store-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = fs::remove_dir_all(&base);
    LAST.with(|l| *l.borrow_mut() = Some(base.clone()));
    (base.join("data"), base.join("docs"))
}

fn lib() -> (Library, PathBuf, PathBuf) {
    let (data, docs) = dirs();
    let (lib, warnings) = Library::open(&data, &docs).unwrap();
    assert!(warnings.is_empty());
    (lib, data, docs)
}

fn files_in(dir: &Path) -> Vec<String> {
    let mut v: Vec<String> = fs::read_dir(dir)
        .unwrap()
        .map(|d| d.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    v.sort();
    v
}

#[test]
fn new_file_is_written_atomically_into_docs() {
    let (mut lib, _, docs) = lib();
    let _cleanup = cleanup();
    let m = lib.create("Макет").unwrap();
    assert_eq!(
        (m.version, m.size, m.disk.as_str()),
        (0.0, 0.0, "Макет.ferrum")
    );
    assert_eq!(lib.read(&m.id).unwrap(), None, "новый — без байтов");
    let w = lib.write(&m.id, b"FERRUM-1", 0.0, None).unwrap();
    assert_eq!((w.version, w.size), (1.0, 8.0));
    assert_eq!(lib.read(&m.id).unwrap().as_deref(), Some(&b"FERRUM-1"[..]));
    lib.write(&m.id, b"FERRUM-22", 1.0, None).unwrap();
    // Только сам файл: временных рядом не осталось.
    assert_eq!(files_in(&docs), vec!["Макет.ferrum".to_string()]);
    // Второе такое же имя — свой файл.
    let other = lib.create("Макет").unwrap();
    assert_eq!(other.disk, "Макет 2.ferrum");
}

#[cfg(unix)]
#[test]
fn failed_write_keeps_the_old_file() {
    use std::os::unix::fs::PermissionsExt;
    let (mut lib, _, docs) = lib();
    let _cleanup = cleanup();
    let m = lib.create("Макет").unwrap();
    lib.write(&m.id, b"OLD", 0.0, None).unwrap();
    // Каталог только для чтения: временный файл не создать.
    fs::set_permissions(&docs, fs::Permissions::from_mode(0o555)).unwrap();
    let err = lib.write(&m.id, b"NEW", 1.0, None);
    fs::set_permissions(&docs, fs::Permissions::from_mode(0o755)).unwrap();
    assert!(err.is_err());
    assert_eq!(fs::read(docs.join("Макет.ferrum")).unwrap(), b"OLD");
    assert_eq!(
        lib.get(&m.id).unwrap().version,
        1.0,
        "поколение не сдвинулось"
    );
}

#[test]
fn stale_generation_and_foreign_program_are_refused() {
    let (mut lib, _, docs) = lib();
    let _cleanup = cleanup();
    let m = lib.create("Макет").unwrap();
    lib.write(&m.id, b"A", 0.0, None).unwrap();
    assert_eq!(lib.write(&m.id, b"B", 0.0, None), Err(CONFLICT.to_string()));
    // Другая программа записала файл.
    std::thread::sleep(std::time::Duration::from_millis(20));
    fs::write(docs.join("Макет.ferrum"), b"CHANGED").unwrap();
    assert_eq!(
        lib.write(&m.id, b"B", 1.0, None),
        Err(DISK_CHANGED.to_string())
    );
    // Открыли заново (прочитали) — запись снова идёт.
    assert_eq!(lib.read(&m.id).unwrap().as_deref(), Some(&b"CHANGED"[..]));
    assert!(lib.write(&m.id, b"B", 1.0, None).is_ok());
    // Удалённая запись: «Файл удалён…».
    lib.delete(&m.id).unwrap();
    assert_eq!(lib.write(&m.id, b"C", 2.0, None), Err(GONE.to_string()));
}

#[test]
fn journal_is_ordered_and_dropped_by_the_snapshot() {
    let (mut lib, ..) = lib();
    let _cleanup = cleanup();
    let m = lib.create("Журнал").unwrap();
    lib.journal_append(&m.id, 20, 1, b"a").unwrap();
    assert!(
        lib.journal_names(&m.id).unwrap().is_empty(),
        "без байтов файла журнала нет"
    );
    lib.write(&m.id, b"F", 0.0, None).unwrap();
    lib.journal_append(&m.id, 20, 2, b"b").unwrap();
    lib.journal_append(&m.id, 10, 7, b"c").unwrap();
    let read = |lib: &Library| -> Vec<Vec<u8>> {
        lib.journal_names(&m.id)
            .unwrap()
            .iter()
            .map(|n| lib.journal_read(&m.id, n).unwrap())
            .collect()
    };
    assert_eq!(
        read(&lib),
        vec![b"c".to_vec(), b"a".to_vec(), b"b".to_vec()]
    );
    lib.write(&m.id, b"F2", 1.0, Some((20, 1))).unwrap();
    assert_eq!(read(&lib), vec![b"b".to_vec()]);
    assert!(
        lib.journal_read(&m.id, "../library.json").is_err(),
        "только свои имена"
    );
}

#[test]
fn sections_search_and_window() {
    let (mut lib, ..) = lib();
    let _cleanup = cleanup();
    let a = lib.create("Альфа").unwrap();
    let b = lib.create("Бета").unwrap();
    lib.create("альфа два").unwrap();
    lib.trash(&b.id, true).unwrap();
    let names = |r: ListResult| r.rows.into_iter().map(|m| m.name).collect::<Vec<_>>();
    assert_eq!(lib.list(Section::Drafts, "", 0, 50).total, 2);
    assert_eq!(names(lib.list(Section::Trash, "", 0, 50)), vec!["Бета"]);
    assert_eq!(lib.list(Section::Drafts, "АЛЬФА", 0, 50).total, 2);
    assert_eq!(lib.list(Section::Drafts, "", 0, 1).rows.len(), 1);
    assert_eq!(lib.list(Section::Drafts, "", 0, 10_000).rows.len(), 2);
    lib.trash(&b.id, false).unwrap();
    assert_eq!(lib.list(Section::Trash, "", 0, 50).total, 0);
    let _ = a;
}

#[test]
fn delete_removes_own_files_but_not_foreign_ones() {
    let (mut lib, _, docs) = lib();
    let _cleanup = cleanup();
    let own = lib.create("Свой").unwrap();
    lib.write(&own.id, b"F", 0.0, None).unwrap();
    let outside = docs.parent().unwrap().join("чужой.ferrum");
    fs::write(&outside, b"FOREIGN").unwrap();
    let foreign = lib.add_existing(&outside).unwrap();
    assert_eq!(foreign.version, 1.0);
    assert_eq!(
        lib.add_existing(&outside).unwrap().id,
        foreign.id,
        "тот же файл — та же запись"
    );
    lib.trash(&own.id, true).unwrap();
    lib.trash(&foreign.id, true).unwrap();
    lib.empty_trash().unwrap();
    assert!(!docs.join("Свой.ferrum").exists());
    assert_eq!(fs::read(&outside).unwrap(), b"FOREIGN");
    assert_eq!(lib.list(Section::Drafts, "", 0, 50).total, 0);
}

#[test]
fn move_to_disk_and_copy() {
    let (mut lib, _, docs) = lib();
    let _cleanup = cleanup();
    let m = lib.create("Макет").unwrap();
    lib.write(&m.id, b"F", 0.0, None).unwrap();
    let target = docs.parent().unwrap().join("на диске.ferrum");
    let moved = lib.move_to(&m.id, &target).unwrap();
    assert_eq!(moved.disk, "на диске.ferrum");
    assert!(!docs.join("Макет.ferrum").exists(), "свой файл переехал");
    lib.write(&m.id, b"F2", 1.0, None).unwrap();
    assert_eq!(fs::read(&target).unwrap(), b"F2");
    let copy = docs.parent().unwrap().join("копия.ferrum");
    lib.copy_to(&m.id, &copy).unwrap();
    assert_eq!(fs::read(&copy).unwrap(), b"F2");
    assert_eq!(
        lib.get(&m.id).unwrap().disk,
        "на диске.ferrum",
        "копия запись не меняет"
    );
}

#[test]
fn library_survives_restart_and_damage() {
    let (data, docs) = dirs();
    let _cleanup = cleanup();
    let id = {
        let (mut lib, _) = Library::open(&data, &docs).unwrap();
        lib.create("Остаётся").unwrap().id
    };
    let (lib, warnings) = Library::open(&data, &docs).unwrap();
    assert!(warnings.is_empty());
    assert_eq!(lib.get(&id).unwrap().name, "Остаётся");
    drop(lib);
    fs::write(data.join("library.json"), b"{ not json").unwrap();
    let (lib, warnings) = Library::open(&data, &docs).unwrap();
    assert_eq!(warnings.len(), 1);
    assert!(lib.get(&id).is_none());
    assert!(data.join("library.json.bad").exists());
}

#[test]
fn settings_are_written_by_field_and_checked() {
    let (data, _) = dirs();
    let _cleanup = cleanup();
    let path = data.join("settings.json");
    let mut s = Settings::open(&path).unwrap();
    let patch = |v: serde_json::Value| v.as_object().unwrap().clone();
    s.set(patch(serde_json::json!({"theme": "light"}))).unwrap();
    s.set(patch(serde_json::json!({"filesView": "list"})))
        .unwrap();
    assert!(s.set(patch(serde_json::json!({"чужое": 1}))).is_err());
    assert!(s
        .set(patch(serde_json::json!({"theme": {"x": 1}})))
        .is_err());
    let big = "x".repeat(90 * 1024);
    assert!(s.set(patch(serde_json::json!({ "keymap": big }))).is_err());
    let again = Settings::open(&path).unwrap();
    assert_eq!(
        serde_json::Value::Object(again.all()),
        serde_json::json!({"theme": "light", "filesView": "list"})
    );
    fs::write(&path, b"[]").unwrap();
    assert!(
        Settings::open(&path).unwrap().all().is_empty(),
        "испорченный — умолчания"
    );
}

#[test]
fn compaction_keeps_later_batches_and_reports_the_highest_session() {
    let (mut lib, ..) = lib();
    let _cleanup = cleanup();
    let m = lib.create("Сжатие").unwrap();
    assert_eq!(lib.read_file(&m.id).unwrap().session, None);
    lib.write(&m.id, b"F", 0.0, None).unwrap();
    lib.journal_append(&m.id, 10, 9, b"old-session").unwrap();
    lib.journal_append(&m.id, 20, 1, b"in").unwrap();
    lib.journal_append(&m.id, 20, 2, b"edge").unwrap();
    lib.journal_append(&m.id, 20, 3, b"later").unwrap();
    lib.journal_append(&m.id, 30, 1, b"next-session").unwrap();
    let r = lib.read_file(&m.id).unwrap();
    assert_eq!((r.bytes.as_deref(), r.session), (Some(&b"F"[..]), Some(30)));
    // Устаревшее поколение: журнал не тронут.
    assert!(lib.write(&m.id, b"X", 0.0, Some((20, 2))).is_err());
    assert_eq!(lib.journal_names(&m.id).unwrap().len(), 5);
    lib.write(&m.id, b"F2", 1.0, Some((20, 2))).unwrap();
    let left: Vec<Vec<u8>> = lib
        .journal_names(&m.id)
        .unwrap()
        .iter()
        .map(|n| lib.journal_read(&m.id, n).unwrap())
        .collect();
    assert_eq!(left, vec![b"later".to_vec(), b"next-session".to_vec()]);
    assert_eq!(lib.journal_session(&m.id).unwrap(), Some(30));
}

#[test]
fn untrusted_input_is_refused_not_panicking() {
    let (mut lib, ..) = lib();
    let _cleanup = cleanup();
    let m = lib.create("Ввод").unwrap();
    lib.write(&m.id, b"F", 0.0, None).unwrap();
    for bad in ["", "../x", "not-a-uuid", &"a".repeat(36), &"-".repeat(36)] {
        assert!(lib.read(bad).is_err());
        assert!(lib.journal_append(bad, 1, 1, b"x").is_err());
        assert!(lib.journal_names(bad).is_err());
        assert!(lib.write_thumb(bad, None).is_err());
        assert_eq!(lib.write(bad, b"x", 0.0, None), Err(GONE.to_string()));
        assert!(lib.get(bad).is_none());
    }
    let too_big = (1u64 << 53) + 1;
    assert!(lib.journal_append(&m.id, too_big, 1, b"x").is_err());
    assert!(lib.journal_append(&m.id, 1, too_big, b"x").is_err());
    assert!(lib.write(&m.id, b"x", 1.0, Some((too_big, 1))).is_err());
    for name in [
        "+0000000000000000001-00000000000000000001.bin",
        "1-1.bin",
        "..",
        "a/b",
        "00000000000000000001-00000000000000000001.bin/../../x",
    ] {
        assert!(lib.journal_read(&m.id, name).is_err(), "{name}");
    }
    let png = vec![0u8; 8 * 1024 * 1024 + 1];
    assert!(lib.write_thumb(&m.id, Some(&png)).is_err());
    // Длинное и управляющее в имени — очищено.
    let long = lib
        .rename(&m.id, &format!("a\u{7}{}", "я".repeat(1000)))
        .unwrap();
    assert_eq!(long.name.chars().count(), 256);
    assert!(!long.name.contains('\u{7}'));
}

#[test]
fn journal_reads_are_capped() {
    let (mut lib, data, _) = lib();
    let _cleanup = cleanup();
    let m = lib.create("Пределы").unwrap();
    lib.write(&m.id, b"F", 0.0, None).unwrap();
    let dir = data.join("journal").join(&m.id);
    fs::create_dir_all(&dir).unwrap();
    // Один большой (разреженный) файл: файл вместе с журналом больше 512 МБ.
    let big = dir.join(format!("{:020}-{:020}.bin", 1, 1));
    fs::File::create(&big)
        .unwrap()
        .set_len(MAX_OPEN_BYTES)
        .unwrap();
    assert!(lib.journal_names(&m.id).unwrap_err().contains("предел"));
    assert!(lib
        .journal_read(&m.id, &format!("{:020}-{:020}.bin", 1, 1))
        .is_ok());
    fs::remove_file(&big).unwrap();
    // Слишком много пачек.
    for i in 0..=MAX_JOURNAL_BATCHES as u64 {
        fs::write(dir.join(format!("{:020}-{:020}.bin", 1, i)), b"").unwrap();
    }
    assert!(lib.journal_names(&m.id).unwrap_err().contains("100000"));
    // Не пачки (чужие имена) не считаются.
    fs::remove_dir_all(&dir).unwrap();
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("мусор.txt"), b"x").unwrap();
    assert!(lib.journal_names(&m.id).unwrap().is_empty());
}

/// Каталог данных только для чтения (unix): индекс не сохранить.
#[cfg(unix)]
fn set_mode(p: &Path, mode: u32) {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(p, fs::Permissions::from_mode(mode)).unwrap();
}

#[cfg(unix)]
#[test]
fn failed_index_save_does_not_advance_the_generation() {
    let (mut lib, data, docs) = lib();
    let _cleanup = cleanup();
    let m = lib.create("Индекс").unwrap();
    lib.write(&m.id, b"A", 0.0, None).unwrap();
    lib.journal_append(&m.id, 1, 1, b"j").unwrap();
    set_mode(&data, 0o555);
    let err = lib.write(&m.id, b"BB", 1.0, Some((1, 1)));
    assert!(err.is_err(), "индекс не записан — отказ");
    assert_eq!(fs::read(docs.join("Индекс.ferrum")).unwrap(), b"BB");
    assert_eq!(lib.get(&m.id).unwrap().version, 1.0, "поколение прежнее");
    assert_eq!(lib.journal_names(&m.id).unwrap().len(), 1, "журнал цел");
    // Другие изменения при том же отказе не остаются в памяти.
    assert!(lib.rename(&m.id, "Другое").is_err());
    assert_eq!(lib.get(&m.id).unwrap().name, "Индекс");
    assert!(lib.trash(&m.id, true).is_err());
    assert!(lib.get(&m.id).unwrap().trashed_at.is_none());
    set_mode(&data, 0o755);
    // Следующая запись проходит: ни CONFLICT, ни DISK_CHANGED.
    let w = lib.write(&m.id, b"CCC", 1.0, Some((1, 1))).unwrap();
    assert_eq!(w.version, 2.0);
    assert!(lib.journal_names(&m.id).unwrap().is_empty());
}

#[test]
fn move_to_the_same_file_keeps_it() {
    let (mut lib, _, docs) = lib();
    let _cleanup = cleanup();
    let m = lib.create("Макет").unwrap();
    lib.write(&m.id, b"F", 0.0, None).unwrap();
    // Тот же файл под другим именем (жёсткая ссылка): копия на себя и удаление «прежнего»
    // стёрли бы единственный файл.
    let alias = docs.join("алиас.ferrum");
    fs::hard_link(docs.join("Макет.ferrum"), &alias).unwrap();
    lib.move_to(&m.id, &alias).unwrap();
    assert_eq!(fs::read(&alias).unwrap(), b"F");
    assert_eq!(fs::read(docs.join("Макет.ferrum")).unwrap(), b"F");
    // Регистр: на файловой системе без учёта регистра (APFS) — тот же файл.
    let upper = docs.join("МАКЕТ.ferrum");
    if upper.exists() {
        lib.move_to(&m.id, &upper).unwrap();
        assert_eq!(fs::read(&upper).unwrap(), b"F");
    }
}

#[test]
fn move_to_another_records_file_is_refused() {
    let (mut lib, _, docs) = lib();
    let _cleanup = cleanup();
    let a = lib.create("А").unwrap();
    let b = lib.create("Б").unwrap();
    lib.write(&a.id, b"A", 0.0, None).unwrap();
    lib.write(&b.id, b"B", 0.0, None).unwrap();
    let err = lib.move_to(&a.id, &docs.join("Б.ferrum")).unwrap_err();
    assert!(err.contains("другой записи"), "{err}");
    assert_eq!(fs::read(docs.join("Б.ferrum")).unwrap(), b"B");
    assert_eq!(lib.get(&a.id).unwrap().disk, "А.ferrum");
}

#[test]
fn reopening_a_trashed_file_restores_it() {
    let (mut lib, _, docs) = lib();
    let _cleanup = cleanup();
    let m = lib.create("Корзина").unwrap();
    lib.write(&m.id, b"F", 0.0, None).unwrap();
    lib.trash(&m.id, true).unwrap();
    let again = lib.add_existing(&docs.join("Корзина.ferrum")).unwrap();
    assert_eq!(again.id, m.id);
    assert!(again.trashed_at.is_none());
    lib.empty_trash().unwrap();
    assert!(
        docs.join("Корзина.ferrum").exists(),
        "открытый файл не стёрт"
    );
}

#[test]
fn open_sweeps_stale_temp_files() {
    let (data, docs) = dirs();
    let _cleanup = cleanup();
    let (mut lib, _) = Library::open(&data, &docs).unwrap();
    let m = lib.create("Х").unwrap();
    lib.write(&m.id, b"F", 0.0, None).unwrap();
    drop(lib);
    let stale = [
        docs.join(".Х.ferrum.ferrum-tmp-1-1"),
        data.join(".library.json.ferrum-tmp-1-1"),
        data.join("thumbs").join(".x.png.ferrum-tmp-1-1"),
    ];
    for p in &stale {
        fs::write(p, b"half").unwrap();
    }
    fs::write(docs.join(".скрытый"), b"mine").unwrap();
    let _ = Library::open(&data, &docs).unwrap();
    for p in &stale {
        assert!(!p.exists(), "{p:?}");
    }
    assert!(docs.join(".скрытый").exists(), "чужое не трогаем");
    assert!(docs.join("Х.ferrum").exists());
}

#[test]
fn long_names_keep_file_and_temp_names_within_255_bytes() {
    let (mut lib, _, docs) = lib();
    let _cleanup = cleanup();
    for ch in ["я", "𝒳"] {
        let m = lib.create(&ch.repeat(200)).unwrap();
        assert!(
            m.disk.len() + ".ferrum-tmp-99999-99999".len() < 255,
            "{}",
            m.disk.len()
        );
        lib.write(&m.id, b"F", 0.0, None).unwrap();
        assert!(docs.join(&m.disk).exists());
    }
}

#[test]
fn external_changes_update_size_and_vanished_file_is_reported() {
    let (mut lib, _, docs) = lib();
    let _cleanup = cleanup();
    let m = lib.create("Внешний").unwrap();
    lib.write(&m.id, b"ABC", 0.0, None).unwrap();
    std::thread::sleep(std::time::Duration::from_millis(20));
    fs::write(docs.join("Внешний.ferrum"), b"LONGER-BYTES").unwrap();
    assert!(lib.read(&m.id).unwrap().is_some());
    assert_eq!(lib.get(&m.id).unwrap().size, 12.0);
    // Другая программа сделала файл пустым: читаем как есть, запись затем не тихая.
    std::thread::sleep(std::time::Duration::from_millis(20));
    fs::write(docs.join("Внешний.ferrum"), b"").unwrap();
    assert_eq!(lib.read(&m.id).unwrap(), None);
    assert_eq!(lib.get(&m.id).unwrap().size, 0.0);
    // Файл исчез: запись не пересоздаёт его молча.
    lib.write(&m.id, b"XY", 1.0, None).unwrap();
    fs::remove_file(docs.join("Внешний.ferrum")).unwrap();
    assert_eq!(
        lib.write(&m.id, b"Z", 2.0, None),
        Err(DISK_MISSING.to_string())
    );
    assert!(!docs.join("Внешний.ferrum").exists());
}

#[test]
fn second_instance_on_the_same_library_is_refused() {
    let (data, docs) = dirs();
    let _cleanup = cleanup();
    let (first, _) = Library::open(&data, &docs).unwrap();
    let err = Library::open(&data, &docs).err().expect("второй экземпляр");
    assert!(err.to_string().contains("уже"), "{err}");
    drop(first);
    assert!(Library::open(&data, &docs).is_ok());
}

#[test]
fn settings_open_drops_unknown_and_non_simple_fields() {
    let (data, _) = dirs();
    let _cleanup = cleanup();
    fs::create_dir_all(&data).unwrap();
    let path = data.join("settings.json");
    fs::write(
        &path,
        serde_json::json!({"theme": "dark", "чужое": "x".repeat(100_000), "keymap": {"a": 1}, "leftWidth": 300})
            .to_string(),
    )
    .unwrap();
    let mut s = Settings::open(&path).unwrap();
    assert_eq!(
        serde_json::Value::Object(s.all()),
        serde_json::json!({"theme": "dark", "leftWidth": 300})
    );
    s.set(
        serde_json::json!({"filesView": "grid"})
            .as_object()
            .unwrap()
            .clone(),
    )
    .unwrap();
}
