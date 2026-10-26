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
import { render } from 'solid-js/web';
import { Root } from './ui/root';

render(() => <Root />, document.getElementById('app')!);
