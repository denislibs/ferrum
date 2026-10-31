// Плавающий тулбар внизу по центру холста (раскладка UI3): перемещение, фрейм с
// пресетами, фигуры (последняя выбранная на кнопке), отключённые перо, текст и
// комментарии — с подсказкой, когда они появятся. Рука (H, удержание Space)
// работает с клавиатуры и в тулбар не выносится.

import { createEffect, createMemo, createSignal, For, onCleanup, onMount, Show } from 'solid-js';
import type { Tool } from '../wasm/ferrum_wasm';
import { useEditor } from './context';
import { Caret, Icon, type IconName } from './icons';
import { Menu, type MenuItem } from './menu';
import { roving } from './roving';

type Shape = 'rect' | 'ellipse' | 'line' | 'image';

const SHAPES: { tool: Shape; name: string; command: string; icon: IconName }[] = [
  { tool: 'rect', name: 'Прямоугольник', command: 'tool.rect', icon: 'rect' },
  { tool: 'ellipse', name: 'Эллипс', command: 'tool.ellipse', icon: 'ellipse' },
  { tool: 'line', name: 'Линия', command: 'tool.line', icon: 'line' },
  { tool: 'image', name: 'Изображение', command: 'tool.image', icon: 'image' },
];

const LATER: { name: string; key: string; when: string; icon: IconName }[] = [
  { name: 'Перо', key: 'P', when: 'подсистема 3', icon: 'pen' },
  { name: 'Комментарии', key: 'C', when: 'подсистема 4', icon: 'comment' },
];

export function Toolbar() {
  const ed = useEditor();
  const [shape, setShape] = createSignal<Shape>('rect');
  const [menu, setMenu] = createSignal<{ kind: 'frame' | 'shapes'; anchor: HTMLElement } | null>(null);
  let bar!: HTMLDivElement;
  onMount(() => onCleanup(roving(bar, 'button')));
  const tool = () => ed.store.state.tool;
  const current = createMemo(() => SHAPES.find((s) => s.tool === shape())!);
  // Фигура, выбранная клавишей, становится и фигурой кнопки.
  createEffect(() => {
    const t = tool() as Tool;
    if (SHAPES.some((s) => s.tool === t)) setShape(t as Shape);
  });

  const setTool = (t: Tool) => ed.send({ type: 'setTool', tool: t });

  const frameItems = (): MenuItem[] => {
    const out: MenuItem[] = [{ label: 'Фрейм', command: 'tool.frame' }];
    let group = '';
    ed.store.state.presets.forEach((p, index) => {
      if (p.group !== group) {
        group = p.group;
        out.push({ separator: true }, { heading: group });
      }
      out.push({
        label: p.name,
        hint: `${p.width} × ${p.height}`,
        onSelect: () => ed.send({ type: 'addFramePreset', index }),
      });
    });
    return out;
  };

  const shapeItems = (): MenuItem[] =>
    SHAPES.map((s) => ({
      label: s.name,
      command: s.command,
      checked: s.tool === shape(),
      onSelect: () => {
        setShape(s.tool);
        setTool(s.tool);
      },
    }));

  return (
    <div ref={bar} class="fe-toolbar" role="toolbar" aria-label="Инструменты">
      <button
        class="fe-tool"
        aria-pressed={tool() === 'move'}
        aria-label="Перемещение"
        data-tip="Перемещение"
        data-command="tool.move"
        data-tip-side="top"
        onClick={() => setTool('move')}
      >
        <Icon name="move" />
      </button>
      <div class="fe-tool-split">
        <button
          class="fe-tool"
          aria-pressed={tool() === 'frame'}
          aria-label="Фрейм"
          data-tip="Фрейм"
          data-command="tool.frame"
          data-tip-side="top"
          onClick={() => setTool('frame')}
        >
          <Icon name="frame" />
        </button>
        <button
          class="fe-tool fe-tool--caret"
          aria-haspopup="menu"
          aria-expanded={menu()?.kind === 'frame'}
          aria-label="Пресеты фреймов"
          data-tip="Пресеты фреймов"
          data-tip-side="top"
          onClick={(e) => setMenu({ kind: 'frame', anchor: e.currentTarget })}
        >
          <Caret />
        </button>
      </div>
      <div class="fe-tool-split">
        <button
          class="fe-tool"
          aria-pressed={tool() === current().tool}
          aria-label={current().name}
          data-tip={current().name}
          data-command={current().command}
          data-tip-side="top"
          onClick={() => setTool(current().tool)}
        >
          <Icon name={current().icon} />
        </button>
        <button
          class="fe-tool fe-tool--caret"
          aria-haspopup="menu"
          aria-expanded={menu()?.kind === 'shapes'}
          aria-label="Фигуры"
          data-tip="Фигуры"
          data-tip-side="top"
          onClick={(e) => setMenu({ kind: 'shapes', anchor: e.currentTarget })}
        >
          <Caret />
        </button>
      </div>
      <button
        class="fe-tool"
        aria-pressed={tool() === 'text'}
        aria-label="Текст"
        data-tip="Текст"
        data-command="tool.text"
        data-tip-side="top"
        onClick={() => setTool('text')}
      >
        <Icon name="text" />
      </button>
      <span class="fe-toolbar__sep" role="separator" />
      <For each={LATER}>
        {(t) => (
          <button
            class="fe-tool"
            aria-disabled="true"
            aria-label={`${t.name} — ${t.when}`}
            data-tip={`${t.name}  ${t.key} — ${t.when}`}
            data-tip-side="top"
          >
            <Icon name={t.icon} />
          </button>
        )}
      </For>
      <Show when={menu()}>
        {(m) => (
          <Menu
            label={m().kind === 'frame' ? 'Пресеты фреймов' : 'Фигуры'}
            items={m().kind === 'frame' ? frameItems() : shapeItems()}
            anchor={m().anchor}
            above
            onClose={() => setMenu(null)}
          />
        )}
      </Show>
    </div>
  );
}
