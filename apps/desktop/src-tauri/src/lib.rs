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

use std::io::ErrorKind;
use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard, OnceLock};

use ferrum_desktop_store::{FileMeta, Library, ListResult, Section, Settings};
use serde::Deserialize;
use serde_json::{Map, Value};
use tauri::ipc::{InvokeBody, Request, Response};
use tauri::{AppHandle, Manager, State, WebviewWindow};
use tauri_plugin_dialog::{DialogExt, MessageDialogKind};

struct Desk {
    lib: Mutex<Library>,
    settings: Mutex<Settings>,
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

/// Байты файла; пустой ответ — новый файл без байтов.
#[tauri::command(async)]
fn files_read(id: String, desk: State<'_, Desk>) -> Res<Response> {
    Ok(Response::new(desk.lib().read(&id)?.unwrap_or_default()))
}

/// Наибольший сеанс среди пачек журнала файла (`null` — пачек нет).
#[tauri::command(async)]
fn files_journal_session(id: String, desk: State<'_, Desk>) -> Res<Option<u64>> {
    desk.lib().journal_session(&id)
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
fn files_journal_names(id: String, desk: State<'_, Desk>) -> Res<Vec<String>> {
    desk.lib().journal_names(&id)
}

#[tauri::command(async)]
fn files_journal_read(id: String, name: String, desk: State<'_, Desk>) -> Res<Response> {
    Ok(Response::new(desk.lib().journal_read(&id, &name)?))
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

#[tauri::command(async)]
fn settings_get(desk: State<'_, Desk>) -> Res<Map<String, Value>> {
    Ok(desk.settings().all())
}

#[tauri::command(async)]
fn settings_set(patch: Map<String, Value>, desk: State<'_, Desk>) -> Res<()> {
    desk.settings().set(patch)
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
    // Ядро запустилось, круг ввода-вывода через IPC прошёл (`io: true`), CSP действует
    // (`cspActive`) и страница его не нарушила (`csp: []`), главный поток во время
    // долгой записи отвечал (`pingMs` меньше половины `writeMs`: команда, ждущая
    // библиотеку в главном потоке, задержала бы ответ почти на всю запись).
    let num = |k: &str| report.get(k).and_then(Value::as_f64).unwrap_or(-1.0);
    let ok = report.get("ready").and_then(Value::as_bool) == Some(true)
        && report.get("io").and_then(Value::as_bool) == Some(true)
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
            "Не удалось открыть библиотеку файлов ({}): {e}",
            data.display()
        ),
    })?;
    for w in warnings {
        eprintln!("{w}");
    }
    let settings_path = config.join("settings.json");
    let settings = Settings::open(&settings_path).map_err(|e| {
        format!(
            "Не удалось открыть настройки ({}): {e}",
            settings_path.display()
        )
    })?;
    Ok(Desk {
        lib: Mutex::new(lib),
        settings: Mutex::new(settings),
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
            files_journal_session,
            files_write,
            files_write_thumb,
            files_thumb,
            files_journal_append,
            files_journal_names,
            files_journal_read,
            files_rename,
            files_touch,
            files_trash,
            files_delete,
            files_empty_trash,
            files_open_dialog,
            files_save_dialog,
            files_copy_dialog,
            settings_get,
            settings_set,
            smoke_mode,
            smoke_done,
        ])
        .run(tauri::generate_context!())
        .expect("Ferrum не запустился");
}
