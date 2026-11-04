// Ferrum: экран файлов и редактор (интерфейс UI3 на SolidJS поверх ядра в
// worker'е); стенд ядра без панелей — debug.html.

import '@fontsource-variable/onest';
import '@fontsource/jetbrains-mono/400.css';
import '@fontsource/jetbrains-mono/500.css';
import './ui/styles/theme.css';
import './ui/styles/base.css';
import './ui/styles/app.css';
import './ui/styles/chrome.css';
import './ui/styles/layers.css';
import './ui/styles/properties.css';
import './ui/styles/files.css';
import './ui/styles/auth.css';
import { render } from 'solid-js/web';
import { AccountRow, AccountTab } from './ui/auth/account';
import { AuthGate } from './ui/auth/auth-gate';
import { extensions } from './ui/extensions';
import { Root } from './ui/root';
import { runSmoke } from './ui/tauri';

// Условие — прямо по `import.meta.env`, без константы: сборщик выбрасывает ветку целиком,
// `scripts/check-dist.mjs` проверяет, что в рабочей сборке её нет.
if (import.meta.env.VITE_FERRUM_LOCAL === '1') {
  document.documentElement.dataset.ferrumLocal = '1';
  render(() => <Root />, document.getElementById('app')!);
} else {
  extensions.accountTab = AccountTab;
  extensions.accountRow = AccountRow;
  render(
    () => (
      <AuthGate>
        <Root />
      </AuthGate>
    ),
    document.getElementById('app')!,
  );
}
// Десктоп с `FERRUM_SMOKE=1`: проверка запуска (вне Tauri ничего не делает).
void runSmoke();
