// Отладочная плашка: бэкенд, транспорт, кадры worker'а и бюджет основного потока.
// Обновляется раз в 250 мс — сама не должна быть заметна в бюджете кадра.

import type { Core } from './core/core';

export function startHud(el: HTMLElement, core: Core): () => void {
  // Последняя ошибка или остановка ядра остаётся на плашке.
  let lastError = '';
  const off = core.on((e) => {
    if (e.type === 'error') lastError = e.message;
  });
  const id = setInterval(() => {
    const l = core.live;
    const main = core.mainThreadStats();
    el.textContent = [
      `бэкенд   ${core.backend ?? '—'}`,
      `транспорт ${core.mode}`,
      `кадры    ${l ? l.fps.toFixed(0) : 0} fps`,
      `кадр     ${l ? l.frameMs.toFixed(1) : 0} мс (p50 ${l ? l.frameP50.toFixed(1) : 0}, p95 ${l ? l.frameP95.toFixed(1) : 0})`,
      `основной ${main.p95Ms.toFixed(2)} мс p95, ${main.maxMs.toFixed(2)} мс макс.`,
      `узлов    ${l ? l.nodes : 0}, зум ${l ? (l.zoom * 100).toFixed(0) : 100}%`,
      ...(lastError ? [`ошибка   ${lastError}`] : []),
    ].join('\n');
  }, 250);
  return () => {
    clearInterval(id);
    off();
  };
}
