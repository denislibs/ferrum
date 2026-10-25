// Тема до первого рисования: читает ту же запись настроек, что `settings.ts`
// (`ferrum.settings.v1`), и ставит `data-theme` на <html> раньше, чем загрузится
// приложение. Отдельный файл, а не встроенный скрипт: не требует 'unsafe-inline'
// в CSP. Любой сбой (хранилище закрыто, мусор в записи) — тёмная, как по умолчанию.
(function () {
  var theme = 'dark';
  try {
    var raw = window.localStorage.getItem('ferrum.settings.v1');
    var choice = raw && raw.length <= 81920 ? JSON.parse(raw).theme : null;
    if (choice === 'light' || choice === 'dark') theme = choice;
    else if (choice === 'system') theme = window.matchMedia('(prefers-color-scheme: dark)').matches ? 'dark' : 'light';
  } catch (e) {
    theme = 'dark';
  }
  document.documentElement.setAttribute('data-theme', theme);
})();
