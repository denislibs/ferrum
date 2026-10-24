// Сгенерировано из crates/ferrum-wasm/src/layout.rs — не править руками.
// Обновить: FERRUM_UPDATE_LAYOUT=1 cargo test -p ferrum-wasm --test layout

export const RECORD_SIZE = 64;
export const RING_CAPACITY = 1024;
export const CTRL_BYTES = 64;
export const LIVE_OFFSET = 64;
export const LIVE_SLOTS = 32;
export const RING_OFFSET = 320;
export const SAB_BYTES = 65856;

export const Ctrl = {
  FRAME: 0,
  INPUT_WRITE: 1,
  INPUT_READ: 2,
  LIVE_SEQ: 3,
  INPUT_DROPPED: 4,
} as const;

export const Kind = {
  POINTER_DOWN: 1,
  POINTER_MOVE: 2,
  POINTER_UP: 3,
  POINTER_CANCEL: 4,
  WHEEL: 5,
  PINCH: 6,
  KEY_DOWN: 7,
  KEY_UP: 8,
  BLUR: 9,
  RESIZE: 10,
} as const;

export const Flag = {
  SHIFT: 1,
  ALT: 2,
  CTRL: 4,
  META: 8,
  ALT_GRAPH: 16,
  REPEAT: 32,
} as const;

export const Field = {
  KIND: 0,
  BUTTON: 1,
  FLAGS: 2,
  CODE_LEN: 3,
  KEY_LEN: 4,
  A: 8,
  B: 16,
  C: 24,
  D: 32,
  CODE: 16,
  KEY: 40,
  TEXT_MAX: 24,
} as const;

export const Live = {
  FRAMES: 0,
  FRAME_MS: 1,
  FRAME_P50: 2,
  FRAME_P95: 3,
  FPS: 4,
  BACKEND: 5,
  SELECTION_COUNT: 6,
  SELECTION_X: 7,
  SELECTION_Y: 8,
  SELECTION_W: 9,
  SELECTION_H: 10,
  SELECTION_ROTATION: 11,
  CAMERA_X: 12,
  CAMERA_Y: 13,
  ZOOM: 14,
  VIEWPORT_W: 15,
  VIEWPORT_H: 16,
  DPR: 17,
  DEVICE_W: 18,
  DEVICE_H: 19,
  BUSY: 20,
  STEPS: 21,
  CAN_UNDO: 22,
  CAN_REDO: 23,
  NODES: 24,
  INPUTS: 25,
  SKIPPED: 26,
  ATLAS_IMAGES: 27,
  ATLAS_MISSES: 28,
} as const;

export const LIVE_NAMES = [
  'frames',
  'frameMs',
  'frameP50',
  'frameP95',
  'fps',
  'backend',
  'selectionCount',
  'selectionX',
  'selectionY',
  'selectionW',
  'selectionH',
  'selectionRotation',
  'cameraX',
  'cameraY',
  'zoom',
  'viewportW',
  'viewportH',
  'dpr',
  'deviceW',
  'deviceH',
  'busy',
  'steps',
  'canUndo',
  'canRedo',
  'nodes',
  'inputs',
  'skipped',
  'atlasImages',
  'atlasMisses',
] as const;

export const BACKENDS = ['none', 'webgpu', 'webgl', 'cpu'] as const;
