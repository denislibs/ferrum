// Тема до первого рисования: зеркало темы `ferrum.theme`, которое пишет хранилище
// настроек (`settings-store.ts`; сами настройки — в IndexedDB, недоступном отсюда
// синхронно), а до первого запуска этапа 6 — прежняя запись `ferrum.settings.v1`.
// Ставит `data-theme` на <html> раньше, чем загрузится приложение. Отдельный файл, а
// не встроенный скрипт: не требует 'unsafe-inline' в CSP. Любой сбой (хранилище
// закрыто, мусор в записи) — тёмная, как по умолчанию.
(function () {
  var theme = 'dark';
  try {
    var choice = window.localStorage.getItem('ferrum.theme');
    if (choice === null) {
      var raw = window.localStorage.getItem('ferrum.settings.v1');
      choice = raw && raw.length <= 81920 ? JSON.parse(raw).theme : null;
    }
    if (choice === 'light' || choice === 'dark') theme = choice;
    else if (choice === 'system') theme = window.matchMedia('(prefers-color-scheme: dark)').matches ? 'dark' : 'light';
  } catch (e) {
    theme = 'dark';
  }
  document.documentElement.setAttribute('data-theme', theme);
})();
