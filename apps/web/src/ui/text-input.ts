// Поле ввода правки текста (спека подсистемы 3, раздел 7): скрытый `<textarea>`
// в основном потоке получает фокус на время правки. Текста документа в нём нет:
// набор (`beforeinput`), составление IME (`composition*`) и клавиши хода курсора
// уходят ядру командами, ядро отвечает прямоугольником курсора (`textCaret`) —
// поле встаёт под курсор, и окно IME появляется у текста. Буфер обмена в правке
// — `system-clipboard.ts` (событие с полем как целью).

import type { Editor } from './context';
import { inputKind, textKeyAction } from '../core/text-keys';

/** Поле правки текста — его узнаёт `system-clipboard.ts`. */
export const TEXT_FIELD_CLASS = 'fe-text-input';

export function isTextField(t: EventTarget | null): boolean {
  return t instanceof HTMLTextAreaElement && t.classList.contains(TEXT_FIELD_CLASS);
}

export function installTextInput(ed: Editor, stage: HTMLElement, canvas: HTMLCanvasElement): () => void {
  const field = document.createElement('textarea');
  field.className = TEXT_FIELD_CLASS;
  field.setAttribute('aria-label', 'Правка текста');
  field.setAttribute('autocomplete', 'off');
  field.setAttribute('autocapitalize', 'off');
  field.setAttribute('spellcheck', 'false');
  field.tabIndex = -1;
  stage.appendChild(field);
  let editing = false;
  let composing = false;
  // Safari шлёт `compositionend` раньше клавиши, которая составление закончила (у неё
  // `isComposing: false`): эта клавиша — IME, а не ход курсора или удаление.
  let justComposed = false;
  let justTimer = 0;

  const send = ed.send.bind(ed);
  const clear = () => {
    if (!composing && field.value !== '') field.value = '';
  };

  const onKeyDown = (e: KeyboardEvent) => {
    if (composing || justComposed) return;
    const action = textKeyAction(
      {
        code: e.code,
        key: e.key,
        ctrlKey: e.ctrlKey,
        metaKey: e.metaKey,
        altKey: e.altKey,
        shiftKey: e.shiftKey,
        isComposing: e.isComposing,
        keyCode: e.keyCode,
        altGraph: e.getModifierState?.('AltGraph') ?? false,
      },
      ed.platform,
    );
    if (action === 'browser') return;
    e.preventDefault();
    // Клавиша обработана здесь: обработчики окна (горячие клавиши ядра) её не видят.
    e.stopPropagation();
    if (action === 'core') {
      send({
        type: 'textKey',
        key: e.code,
        mods: { shift: e.shiftKey, alt: e.altKey, ctrl: e.ctrlKey, meta: e.metaKey },
      });
    }
  };
  const onBeforeInput = (e: InputEvent) => {
    // Составление IME браузер не даёт отменить: его ведут события составления.
    if (e.inputType === 'insertCompositionText' || e.isComposing) return;
    e.preventDefault();
    if (e.inputType === 'historyUndo' || e.inputType === 'historyRedo') {
      send({ type: 'textKey', key: 'KeyZ', mods: { shift: e.inputType === 'historyRedo', ctrl: ed.platform !== 'mac', meta: ed.platform === 'mac', alt: false } });
      return;
    }
    const kind = inputKind(e.inputType);
    if (!kind) return;
    send({ type: 'textInput', kind, data: e.data ?? undefined });
  };
  const onCompositionStart = () => {
    composing = true;
  };
  const onCompositionUpdate = (e: CompositionEvent) => {
    send({ type: 'textInput', kind: 'compose', data: e.data });
  };
  const onCompositionEnd = (e: CompositionEvent) => {
    composing = false;
    justComposed = true;
    window.clearTimeout(justTimer);
    justTimer = window.setTimeout(() => (justComposed = false), 0);
    send({ type: 'textInput', kind: 'commit', data: e.data });
    field.value = '';
  };
  const onInput = () => clear();
  field.addEventListener('keydown', onKeyDown);
  field.addEventListener('beforeinput', onBeforeInput);
  field.addEventListener('compositionstart', onCompositionStart);
  field.addEventListener('compositionupdate', onCompositionUpdate);
  field.addEventListener('compositionend', onCompositionEnd);
  field.addEventListener('input', onInput);
  // Сцена не прокручивается (overflow: hidden не мешает прокрутке к полю).
  const onStageScroll = () => {
    if (stage.scrollTop !== 0) stage.scrollTop = 0;
    if (stage.scrollLeft !== 0) stage.scrollLeft = 0;
  };
  stage.addEventListener('scroll', onStageScroll);

  const off = ed.core.on((e) => {
    if (e.type !== 'textCaret') return;
    if (e.editing && e.rect) {
      // Поле — под курсором (CSS-пиксели холста; холст — в начале сцены).
      const [x, y, , h] = e.rect;
      // Не за краем сцены: иначе браузер прокрутит её к полю при составлении.
      const fh = Math.min(Math.max(1, Math.round(h)), Math.max(1, stage.clientHeight));
      const fx = Math.min(Math.max(0, Math.round(x)), Math.max(0, stage.clientWidth - 1));
      const fy = Math.min(Math.max(0, Math.round(y)), Math.max(0, stage.clientHeight - fh));
      field.style.transform = `translate(${fx}px, ${fy}px)`;
      field.style.height = `${fh}px`;
      ed.core.keyTarget = field;
      // Фокус — полю, если он не в панели (правка шрифта из панели не прерывает правку).
      const active = document.activeElement;
      if (!editing || active === canvas || active === document.body || active === null) {
        field.focus({ preventScroll: true });
      }
      editing = true;
    } else if (editing) {
      editing = false;
      ed.core.keyTarget = null;
      composing = false;
      field.value = '';
      if (document.activeElement === field) canvas.focus({ preventScroll: true });
    }
  });

  return () => {
    off();
    ed.core.keyTarget = null;
    field.removeEventListener('keydown', onKeyDown);
    field.removeEventListener('beforeinput', onBeforeInput);
    field.removeEventListener('compositionstart', onCompositionStart);
    field.removeEventListener('compositionupdate', onCompositionUpdate);
    field.removeEventListener('compositionend', onCompositionEnd);
    field.removeEventListener('input', onInput);
    stage.removeEventListener('scroll', onStageScroll);
    window.clearTimeout(justTimer);
    field.remove();
  };
}
