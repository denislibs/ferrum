//! Десктоп Ferrum (этап 6, спека 10): окно с интерфейсом `apps/web` и команды IPC
//! хранилища на диске (`ferrum-desktop-store`): список файлов, чтение и атомарная
//! запись, журнал, превью, настройки в файле конфигурации, родные диалоги «Открыть»
//! и «Сохранить». Байты идут сырым телом IPC (без JSON): файл — `Uint8Array` в
//! запросе и `ArrayBuffer` в ответе; параметры записи — заголовком `x-ferrum`.
//!
//! Интерфейс не доверенный: id — только UUID из библиотеки, пути — только из родных
//! диалогов (интерфейс их не передаёт), имена пачек журнала — только своего вида.
//!
//! Библиотека и настройки — по одному на процесс, за `Mutex`. Все команды, которые
//! берут `Mutex` или трогают диск, — `async`: они идут в потоках Tauri, а не в главном
//! потоке окна (синхронная команда Tauri выполняется в нём и, ожидая `Mutex` под
//! записью с `fsync`, остановила бы окно). Синхронна только `smoke_mode` — по ней
//! проверка запуска и меряет, свободен ли главный поток во время долгой записи.
//! Второй экземпляр приложения библиотеку не откроет (`library.lock`): он показывает
//! причину родным окном сообщения и выходит.
//!
//! Проверка запуска (`FERRUM_SMOKE=1`): интерфейс открывает новый файл, дожидается
//! ядра, проходит круг ввода-вывода через IPC и сообщает итог командой `smoke_done`;
//! приложение печатает его, убирает свой временный каталог и выходит (код 0 — ядро
//! запустилось и круг прошёл), без ответа за 90 с — выход с кодом 1.

mod fonts;

use std::io::ErrorKind;
use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard, OnceLock};

use ferrum_fontmeta::FamilyInfo;

use ferrum_desktop_store::{io_error, FileMeta, Library, ListResult, ReadParts, Section, Settings};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use tauri::ipc::{InvokeBody, Request, Response};
use tauri::{AppHandle, Manager, State, WebviewWindow};
use tauri_plugin_dialog::{DialogExt, MessageDialogKind};

struct Desk {
    lib: Mutex<Library>,
    settings: Mutex<Settings>,
    /// Части чтений файла, ещё не забранные интерфейсом (`files_read_part`).
    parts: Mutex<ReadParts>,
    /// Системные шрифты (подсистема 3): сканируются при первом `fonts_list`.
    fonts: OnceLock<fonts::SystemFonts>,
}

type Res<T> = Result<T, String>;

const BAD: &str = "Неверный запрос к хранилищу файлов";
/// Диалог вернул не путь в файловой системе (например, ссылку).
const NOT_A_PATH: &str = "Выбранный файл недоступен: это не файл на диске";

impl Desk {
    fn lib(&self) -> MutexGuard<'_, Library> {
        self.lib.lock().unwrap_or_else(|p| p.into_inner())
    }
    fn settings(&self) -> MutexGuard<'_, Settings> {
        self.settings.lock().unwrap_or_else(|p| p.into_inner())
    }
    fn parts(&self) -> MutexGuard<'_, ReadParts> {
        self.parts.lock().unwrap_or_else(|p| p.into_inner())
    }
}

/// Сырое тело запроса (без копии: до 512 МБ) и параметры из заголовка `x-ferrum`
/// (JSON только из ASCII: id — UUID, числа).
fn raw<'r, T: for<'de> Deserialize<'de>>(request: &'r Request<'_>) -> Res<(&'r [u8], T)> {
    let InvokeBody::Raw(bytes) = request.body() else {
        return Err(BAD.into());
    };
    let args = request
        .headers()
        .get("x-ferrum")
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| BAD.to_string())?;
    let args = serde_json::from_str(args).map_err(|_| BAD.to_string())?;
    Ok((bytes, args))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct WriteArgs {
    id: String,
    base: f64,
    session: Option<u64>,
    up_to: Option<u64>,
}

#[derive(Deserialize)]
struct ExportArgs {
    name: String,
}

#[derive(Deserialize)]
struct ThumbArgs {
    id: String,
}

#[derive(Deserialize)]
struct JournalArgs {
    id: String,
    session: u64,
    batch: u64,
}

#[tauri::command(async)]
fn files_list(
    section: Section,
    query: String,
    start: usize,
    count: usize,
    desk: State<'_, Desk>,
) -> Res<ListResult> {
    Ok(desk.lib().list(section, &query, start, count))
}

#[tauri::command(async)]
fn files_get(id: String, desk: State<'_, Desk>) -> Res<Option<FileMeta>> {
    Ok(desk.lib().get(&id))
}

#[tauri::command(async)]
fn files_create(name: String, desk: State<'_, Desk>) -> Res<FileMeta> {
    desk.lib().create(&name)
}

/// Байты файла («Скачать копию»); пустой ответ — новый файл без байтов.
#[tauri::command(async)]
fn files_read(id: String, desk: State<'_, Desk>) -> Res<Response> {
    Ok(Response::new(desk.lib().read(&id)?.unwrap_or_default()))
}

/// Ответ `files_read_open`: всё, кроме байтов; байты — `parts` частей чтения `ticket`
/// (сначала файл, если `bytes`, затем пачки журнала по порядку).
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ReadHead {
    meta: FileMeta,
    session: Option<u64>,
    ticket: u64,
    parts: usize,
    bytes: bool,
    journal_dropped: bool,
}

/// Открытие файла: метаданные, байты, сеанс и журнал — одним снимком под одной
/// блокировкой библиотеки (запись между частями чтения не смешает старые байты с
/// новым поколением). Байты ждут в `parts`, интерфейс забирает их по одной.
#[tauri::command(async)]
fn files_read_open(id: String, desk: State<'_, Desk>) -> Res<ReadHead> {
    let r = desk.lib().read_file(&id)?;
    let bytes = r.bytes.is_some();
    let mut parts: Vec<Vec<u8>> = r.bytes.into_iter().collect();
    parts.extend(r.journal);
    let count = parts.len();
    let ticket = desk.parts().put(parts);
    Ok(ReadHead {
        meta: r.meta,
        session: r.session,
        ticket,
        parts: count,
        bytes,
        journal_dropped: r.journal_dropped,
    })
}

/// Следующая часть чтения `ticket` — сырым ответом (`ArrayBuffer` в окне).
#[tauri::command(async)]
fn files_read_part(ticket: u64, desk: State<'_, Desk>) -> Res<Response> {
    Ok(Response::new(desk.parts().take(ticket)?))
}

#[tauri::command(async)]
fn files_write(request: Request<'_>, desk: State<'_, Desk>) -> Res<FileMeta> {
    let (bytes, a): (&[u8], WriteArgs) = raw(&request)?;
    let journal = a.session.zip(a.up_to);
    desk.lib().write(&a.id, bytes, a.base, journal)
}

/// Превью: пустое тело — убрать.
#[tauri::command(async)]
fn files_write_thumb(request: Request<'_>, desk: State<'_, Desk>) -> Res<FileMeta> {
    let (bytes, a): (&[u8], ThumbArgs) = raw(&request)?;
    desk.lib()
        .write_thumb(&a.id, (!bytes.is_empty()).then_some(bytes))
}

#[tauri::command(async)]
fn files_thumb(id: String, desk: State<'_, Desk>) -> Res<Response> {
    Ok(Response::new(desk.lib().thumb(&id).unwrap_or_default()))
}

#[tauri::command(async)]
fn files_journal_append(request: Request<'_>, desk: State<'_, Desk>) -> Res<()> {
    let (bytes, a): (&[u8], JournalArgs) = raw(&request)?;
    desk.lib().journal_append(&a.id, a.session, a.batch, bytes)
}

#[tauri::command(async)]
fn files_rename(id: String, name: String, desk: State<'_, Desk>) -> Res<FileMeta> {
    desk.lib().rename(&id, &name)
}

#[tauri::command(async)]
fn files_touch(id: String, desk: State<'_, Desk>) -> Res<FileMeta> {
    desk.lib().touch(&id)
}

#[tauri::command(async)]
fn files_trash(id: String, on: bool, desk: State<'_, Desk>) -> Res<()> {
    desk.lib().trash(&id, on)
}

#[tauri::command(async)]
fn files_delete(id: String, desk: State<'_, Desk>) -> Res<()> {
    desk.lib().delete(&id)
}

#[tauri::command(async)]
fn files_empty_trash(desk: State<'_, Desk>) -> Res<()> {
    desk.lib().empty_trash()
}

/// «Открыть с диска…»: родной диалог; запись для выбранного файла. `None` — отказ.
#[tauri::command]
async fn files_open_dialog(
    app: AppHandle,
    window: WebviewWindow,
    desk: State<'_, Desk>,
) -> Res<Option<FileMeta>> {
    let Some(picked) = app
        .dialog()
        .file()
        .set_parent(&window)
        .add_filter("Файл Ferrum", &["ferrum"])
        .blocking_pick_file()
    else {
        return Ok(None);
    };
    let path = picked.into_path().map_err(|_| NOT_A_PATH.to_string())?;
    desk.lib().add_existing(&path).map(Some)
}

/// «Сохранить на диск…»: файл записи переезжает по выбранному пути.
#[tauri::command]
async fn files_save_dialog(
    id: String,
    name: String,
    app: AppHandle,
    window: WebviewWindow,
    desk: State<'_, Desk>,
) -> Res<Option<FileMeta>> {
    let Some(picked) = app
        .dialog()
        .file()
        .set_parent(&window)
        .set_file_name(&name)
        .add_filter("Файл Ferrum", &["ferrum"])
        .blocking_save_file()
    else {
        return Ok(None);
    };
    let path = picked.into_path().map_err(|_| NOT_A_PATH.to_string())?;
    desk.lib().move_to(&id, &path).map(Some)
}

/// «Сохранить копию на диск…»: копия по выбранному пути; `false` — отказ.
#[tauri::command]
async fn files_copy_dialog(
    id: String,
    name: String,
    app: AppHandle,
    window: WebviewWindow,
    desk: State<'_, Desk>,
) -> Res<bool> {
    let Some(picked) = app
        .dialog()
        .file()
        .set_parent(&window)
        .set_file_name(&name)
        .add_filter("Файл Ferrum", &["ferrum"])
        .blocking_save_file()
    else {
        return Ok(false);
    };
    let path = picked.into_path().map_err(|_| NOT_A_PATH.to_string())?;
    desk.lib().copy_to(&id, &path).map(|()| true)
}

/// Подпись фильтра и расширение файла экспорта по имени (`.png`, `.svg`, `.pdf`).
fn export_kind(name: &str) -> Option<(&'static str, &'static str)> {
    let (_, ext) = name.rsplit_once('.')?;
    match ext.to_ascii_lowercase().as_str() {
        "png" => Some(("Изображение PNG", "png")),
        "svg" => Some(("Изображение SVG", "svg")),
        "pdf" => Some(("Документ PDF", "pdf")),
        _ => None,
    }
}

/// Имя файла экспорта для диалога: без разделителей пути и управляющих
/// символов, не длиннее 255 символов.
fn export_name(name: &str) -> String {
    name.chars()
        .filter(|c| !c.is_control())
        .map(|c| {
            if matches!(c, '/' | '\\' | ':') {
                '_'
            } else {
                c
            }
        })
        .take(255)
        .collect()
}

/// Экспорт PNG, SVG и PDF (подсистема 3, этап 5): родной диалог «Сохранить» с
/// именем из интерфейса и фильтром по типу; байты — сырым телом запроса, запись
/// атомарная. `false` — отказ в диалоге. Путь интерфейс не видит.
#[tauri::command(async)]
fn export_save(request: Request<'_>, app: AppHandle, window: WebviewWindow) -> Res<bool> {
    let (bytes, a): (&[u8], ExportArgs) = raw(&request)?;
    let name = export_name(&a.name);
    let (label, ext) = export_kind(&name).ok_or_else(|| BAD.to_string())?;
    let Some(picked) = app
        .dialog()
        .file()
        .set_parent(&window)
        .set_file_name(&name)
        .add_filter(label, &[ext])
        .blocking_save_file()
    else {
        return Ok(false);
    };
    let path = picked.into_path().map_err(|_| NOT_A_PATH.to_string())?;
    ferrum_desktop_store::atomic_write(&path, bytes)
        .map_err(|e| format!("Не удалось сохранить файл: {}", io_error(&e)))?;
    Ok(true)
}

#[tauri::command(async)]
fn settings_get(desk: State<'_, Desk>) -> Res<Map<String, Value>> {
    Ok(desk.settings().all())
}

#[tauri::command(async)]
fn settings_set(patch: Map<String, Value>, desk: State<'_, Desk>) -> Res<()> {
    desk.settings().set(patch)
}

/// Системные шрифты: семейства и начертания (файл — номер в списке процесса).
/// Первый вызов сканирует каталоги шрифтов ОС — в потоке Tauri, не в окне.
#[tauri::command(async)]
fn fonts_list(desk: State<'_, Desk>) -> Res<Vec<FamilyInfo>> {
    let fonts = desk.fonts.get_or_init(|| {
        let f = fonts::SystemFonts::scan(&fonts::font_dirs());
        for s in &f.skipped {
            eprintln!("{s}");
        }
        f
    });
    Ok(fonts.families.clone())
}

/// Байты файла системного шрифта сырым ответом (без JSON и копий в JS).
#[tauri::command(async)]
fn fonts_read(id: usize, desk: State<'_, Desk>) -> Res<Response> {
    let fonts = desk
        .fonts
        .get()
        .ok_or("Список системных шрифтов ещё не построен")?;
    Ok(Response::new(fonts.read(id)?))
}

fn smoke() -> bool {
    std::env::var("FERRUM_SMOKE").is_ok_and(|v| v == "1")
}

/// Временный каталог проверки запуска (библиотека, документы, настройки).
static SMOKE_DIR: OnceLock<PathBuf> = OnceLock::new();

/// Убрать временный каталог проверки (перед выходом).
fn smoke_cleanup() {
    if let Some(dir) = SMOKE_DIR.get() {
        let _ = std::fs::remove_dir_all(dir);
    }
}

#[tauri::command]
fn smoke_mode() -> bool {
    smoke()
}

/// Итог проверки запуска; вне режима проверки — отказ (страница не может закрыть
/// приложение).
#[tauri::command]
fn smoke_done(report: Value, app: AppHandle) -> Res<()> {
    if !smoke() {
        return Err("Команда доступна только при проверке запуска".into());
    }
    println!("smoke: {report}");
    // Ядро запустилось, круг ввода-вывода через IPC прошёл (`io: true`), экран файлов
    // показал превью и редкие символы со своими шрифтами (`screen: true`), CSP действует
    // (`cspActive`) и страница его не нарушила (`csp: []`), главный поток во время
    // долгой записи отвечал (`pingMs` меньше половины `writeMs`: команда, ждущая
    // библиотеку в главном потоке, задержала бы ответ почти на всю запись).
    let num = |k: &str| report.get(k).and_then(Value::as_f64).unwrap_or(-1.0);
    let ok = report.get("ready").and_then(Value::as_bool) == Some(true)
        && report.get("io").and_then(Value::as_bool) == Some(true)
        && report.get("screen").and_then(Value::as_bool) == Some(true)
        && report.get("cspActive").and_then(Value::as_bool) == Some(true)
        && report
            .get("csp")
            .and_then(Value::as_array)
            .is_some_and(Vec::is_empty)
        && num("pingMs") >= 0.0
        && num("pingMs") < num("writeMs") / 2.0;
    smoke_cleanup();
    app.exit(if ok { 0 } else { 1 });
    Ok(())
}

/// Библиотека и настройки: проверка запуска — во временном каталоге (библиотека
/// пользователя не трогается).
fn open_desk(app: &tauri::App) -> Result<Desk, String> {
    let (data, docs, config) = if smoke() {
        let tmp = SMOKE_DIR
            .get_or_init(|| {
                std::env::temp_dir().join(format!("ferrum-smoke-{}", std::process::id()))
            })
            .clone();
        (tmp.join("data"), tmp.join("docs"), tmp.join("config"))
    } else {
        // Отказ системы назвать каталог — без английского текста ошибки.
        let p = app.path();
        let none = |what: &str| format!("Не найден {what}: система не назвала его");
        (
            p.app_data_dir()
                .map_err(|_| none("каталог данных приложения"))?,
            p.document_dir()
                .map_err(|_| none("каталог «Документы»"))?
                .join("Ferrum"),
            p.app_config_dir()
                .map_err(|_| none("каталог настроек приложения"))?,
        )
    };
    // Второй экземпляр — свой русский текст хранилища; прочее — с причиной от системы.
    let (lib, warnings) = Library::open(&data, &docs).map_err(|e| match e.kind() {
        ErrorKind::WouldBlock => e.to_string(),
        _ => format!(
            "Не удалось открыть библиотеку файлов ({}): {}",
            data.display(),
            io_error(&e)
        ),
    })?;
    for w in warnings {
        eprintln!("{w}");
    }
    let settings_path = config.join("settings.json");
    let settings = Settings::open(&settings_path).map_err(|e| {
        format!(
            "Не удалось открыть настройки ({}): {}",
            settings_path.display(),
            io_error(&e)
        )
    })?;
    Ok(Desk {
        lib: Mutex::new(lib),
        settings: Mutex::new(settings),
        parts: Mutex::new(ReadParts::default()),
        fonts: OnceLock::new(),
    })
}

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            if smoke() {
                std::thread::spawn(|| {
                    std::thread::sleep(std::time::Duration::from_secs(90));
                    println!("smoke: timeout");
                    smoke_cleanup();
                    std::process::exit(1);
                });
            }
            match open_desk(app) {
                Ok(desk) => {
                    app.manage(desk);
                    // Окно скрыто до открытия библиотеки: без неё интерфейсу нечего показать.
                    if let Some(w) = app.get_webview_window("main") {
                        w.show()?;
                    }
                }
                Err(message) => {
                    // Библиотека не открылась (например, Ferrum уже запущен): причина —
                    // родным окном сообщения, затем выход; окно редактора не показывается.
                    eprintln!("{message}");
                    if smoke() {
                        println!("smoke: {}", Value::String(message.clone()));
                        smoke_cleanup();
                        std::process::exit(1);
                    }
                    let handle = app.handle().clone();
                    app.dialog()
                        .message(message)
                        .title("Ferrum не запустился")
                        .kind(MessageDialogKind::Error)
                        .show(move |_| handle.exit(1));
                }
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            files_list,
            files_get,
            files_create,
            files_read,
            files_read_open,
            files_read_part,
            files_write,
            files_write_thumb,
            files_thumb,
            files_journal_append,
            files_rename,
            files_touch,
            files_trash,
            files_delete,
            files_empty_trash,
            files_open_dialog,
            files_save_dialog,
            files_copy_dialog,
            export_save,
            settings_get,
            settings_set,
            fonts_list,
            fonts_read,
            smoke_mode,
            smoke_done,
        ])
        .run(tauri::generate_context!())
        .expect("Ferrum не запустился");
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Тип экспорта — только по расширению PNG, SVG и PDF; имя без путей.
    #[test]
    fn export_names_are_checked_and_cleaned() {
        assert_eq!(export_kind("Экран.svg"), Some(("Изображение SVG", "svg")));
        assert_eq!(export_kind("a@2x.PNG"), Some(("Изображение PNG", "png")));
        assert_eq!(export_kind("Отчёт.pdf").map(|k| k.1), Some("pdf"));
        assert_eq!(export_kind("файл.ferrum"), None);
        assert_eq!(export_kind("без расширения"), None);
        assert_eq!(export_name("../a/b\\c:d\u{0}.svg"), ".._a_b_c_d.svg");
        assert_eq!(export_name(&"я".repeat(400)).chars().count(), 255);
    }
}
