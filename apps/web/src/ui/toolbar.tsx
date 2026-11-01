// Плавающий тулбар внизу по центру холста (раскладка UI3): перемещение и масштаб
// (K) со стрелкой (как в Figma: последний выбранный на кнопке), фрейм с
// пресетами, фигуры со стрелкой (последняя выбранная на кнопке), перо, текст,
// булевы операции со стрелкой (как в Figma: операция выделенной булевой группы
// или последняя выбранная на кнопке, в списке — ещё «Сплющить» и «Контур
// обводки») и отключённые комментарии — с подсказкой, когда они появятся. Рука (H, удержание
// Space) работает с клавиатуры и в тулбар не выносится.

import { createEffect, createMemo, createSignal, For, onCleanup, onMount, Show } from 'solid-js';
import type { BooleanOpDto, Tool } from '../wasm/ferrum_wasm';
import { useEditor } from './context';
import { Caret, Icon, type IconName } from './icons';
import { Menu, type MenuItem } from './menu';
import { roving } from './roving';

type Shape = 'rect' | 'ellipse' | 'line' | 'arrow' | 'image';
type Mover = 'move' | 'scale';

const MOVERS: { tool: Mover; name: string; command: string; icon: IconName }[] = [
  { tool: 'move', name: 'Перемещение', command: 'tool.move', icon: 'move' },
  { tool: 'scale', name: 'Масштабирование', command: 'tool.scale', icon: 'scale' },
];

const SHAPES: { tool: Shape; name: string; command: string; icon: IconName }[] = [
  { tool: 'rect', name: 'Прямоугольник', command: 'tool.rect', icon: 'rect' },
  { tool: 'ellipse', name: 'Эллипс', command: 'tool.ellipse', icon: 'ellipse' },
  { tool: 'line', name: 'Линия', command: 'tool.line', icon: 'line' },
  { tool: 'arrow', name: 'Стрелка', command: 'tool.arrow', icon: 'arrow' },
  { tool: 'image', name: 'Изображение', command: 'tool.image', icon: 'image' },
];

const BOOLEANS: { op: BooleanOpDto; name: string; command: string }[] = [
  { op: 'union', name: 'Объединение', command: 'object.boolean_union' },
  { op: 'subtract', name: 'Вычитание', command: 'object.boolean_subtract' },
  { op: 'intersect', name: 'Пересечение', command: 'object.boolean_intersect' },
  { op: 'exclude', name: 'Исключение', command: 'object.boolean_exclude' },
];

const LATER: { name: string; key: string; when: string; icon: IconName }[] = [
  { name: 'Комментарии', key: 'C', when: 'подсистема 4', icon: 'comment' },
];

export function Toolbar() {
  const ed = useEditor();
  const [shape, setShape] = createSignal<Shape>('rect');
  const [mover, setMover] = createSignal<Mover>('move');
  type MenuKind = 'move' | 'frame' | 'shapes' | 'boolean';
  const [menu, setMenu] = createSignal<{ kind: MenuKind; anchor: HTMLElement } | null>(null);
  // Состояние команд — из ядра: доступна ли операция и какая отмечена у
  // выделенных булевых групп (основной поток ничего не вычисляет).
  const command = (key: string) => ed.store.state.commands[key];
  const enabled = (key: string) => command(key)?.enabled ?? false;
  // Операция на кнопке: отмеченная у выделенных групп, иначе последняя выбранная.
  const [lastOp, setLastOp] = createSignal<BooleanOpDto>('union');
  const selectedOp = () => BOOLEANS.find((b) => command(b.command)?.checked);
  const boolean = createMemo(() => selectedOp() ?? BOOLEANS.find((b) => b.op === lastOp())!);
  const runBoolean = (b: (typeof BOOLEANS)[number]) => {
    setLastOp(b.op);
    ed.run(b.command);
  };
  let bar!: HTMLDivElement;
  onMount(() => onCleanup(roving(bar, 'button')));
  const tool = () => ed.store.state.tool;
  const current = createMemo(() => SHAPES.find((s) => s.tool === shape())!);
  const currentMover = createMemo(() => MOVERS.find((m) => m.tool === mover())!);
  // Фигура (и перемещение или масштаб), выбранные клавишей, становятся и на кнопке.
  createEffect(() => {
    const t = tool() as Tool;
    if (SHAPES.some((s) => s.tool === t)) setShape(t as Shape);
    if (MOVERS.some((m) => m.tool === t)) setMover(t as Mover);
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

  const moverItems = (): MenuItem[] =>
    MOVERS.map((m) => ({
      label: m.name,
      command: m.command,
      checked: m.tool === mover(),
      onSelect: () => {
        setMover(m.tool);
        setTool(m.tool);
      },
    }));

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

  // Доступность и отметка пунктов — из состояния команд (переключатели ядра).
  const booleanItems = (): MenuItem[] => [
    ...BOOLEANS.map((b) => ({
      label: b.name,
      command: b.command,
      onSelect: () => runBoolean(b),
    })),
    { separator: true as const },
    { label: 'Сплющить', command: 'object.flatten' },
    { label: 'Контур обводки', command: 'object.outline_stroke' },
  ];

  const menuLabel = (kind: MenuKind) =>
    kind === 'move'
      ? 'Инструменты перемещения'
      : kind === 'frame'
        ? 'Пресеты фреймов'
        : kind === 'shapes'
          ? 'Фигуры'
          : 'Булевы операции';
  const menuItems = (kind: MenuKind) =>
    kind === 'move' ? moverItems() : kind === 'frame' ? frameItems() : kind === 'shapes' ? shapeItems() : booleanItems();

  return (
    <div ref={bar} class="fe-toolbar" role="toolbar" aria-label="Инструменты">
      <div class="fe-tool-split">
        <button
          class="fe-tool"
          aria-pressed={tool() === currentMover().tool}
          aria-label={currentMover().name}
          data-tip={currentMover().name}
          data-command={currentMover().command}
          data-tip-side="top"
          onClick={() => setTool(currentMover().tool)}
        >
          <Icon name={currentMover().icon} />
        </button>
        <button
          class="fe-tool fe-tool--caret"
          aria-haspopup="menu"
          aria-expanded={menu()?.kind === 'move'}
          aria-label="Инструменты перемещения"
          data-tip="Инструменты перемещения"
          data-tip-side="top"
          onClick={(e) => setMenu({ kind: 'move', anchor: e.currentTarget })}
        >
          <Caret />
        </button>
      </div>
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
        aria-pressed={tool() === 'pen'}
        aria-label="Перо"
        data-tip="Перо"
        data-command="tool.pen"
        data-tip-side="top"
        onClick={() => setTool('pen')}
      >
        <Icon name="pen" />
      </button>
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
      <div class="fe-tool-split">
        <button
          class="fe-tool"
          aria-disabled={!enabled(boolean().command)}
          aria-pressed={selectedOp() !== undefined}
          aria-label={boolean().name}
          data-tip={boolean().name}
          data-command={boolean().command}
          data-tip-side="top"
          onClick={() => enabled(boolean().command) && runBoolean(boolean())}
        >
          <Icon name={boolean().op} />
        </button>
        <button
          class="fe-tool fe-tool--caret"
          aria-haspopup="menu"
          aria-expanded={menu()?.kind === 'boolean'}
          aria-label="Булевы операции"
          data-tip="Булевы операции"
          data-tip-side="top"
          onClick={(e) => setMenu({ kind: 'boolean', anchor: e.currentTarget })}
        >
          <Caret />
        </button>
      </div>
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
            label={menuLabel(m().kind)}
            items={menuItems(m().kind)}
            anchor={m().anchor}
            above
            onClose={() => setMenu(null)}
          />
        )}
      </Show>
    </div>
  );
}
