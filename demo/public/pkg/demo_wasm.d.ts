/* tslint:disable */
/* eslint-disable */

export function on_key_down(key_str: string, shift: boolean, ctrl: boolean, alt: boolean): void;

export function on_mouse_down(x: number, y: number, button: number): void;

export function on_mouse_leave(): void;

export function on_mouse_move(x: number, y: number): void;

export function on_mouse_up(x: number, y: number, button: number): void;

export function on_mouse_wheel(x: number, y: number, delta_y: number): void;

/**
 * Full-frame render.  Direct GL path: the widget tree is painted via
 * `GlGfxCtx` (tess2 tessellation → WebGL2 draw calls).  No off-screen
 * framebuffer is used.  The rotating 3D cube is drawn last, on top.
 */
export function render(width: number, height: number): void;

/**
 * Initialise panic hook so Rust panics appear in the browser console.
 */
export function wasm_start(): void;

export type InitInput = RequestInfo | URL | Response | BufferSource | WebAssembly.Module;

export interface InitOutput {
    readonly memory: WebAssembly.Memory;
    readonly render: (a: number, b: number) => void;
    readonly on_mouse_down: (a: number, b: number, c: number) => void;
    readonly on_mouse_up: (a: number, b: number, c: number) => void;
    readonly on_key_down: (a: number, b: number, c: number, d: number, e: number) => void;
    readonly wasm_start: () => void;
    readonly on_mouse_leave: () => void;
    readonly on_mouse_move: (a: number, b: number) => void;
    readonly on_mouse_wheel: (a: number, b: number, c: number) => void;
    readonly __wbindgen_malloc: (a: number, b: number) => number;
    readonly __wbindgen_realloc: (a: number, b: number, c: number, d: number) => number;
    readonly __externref_table_alloc: () => number;
    readonly __wbindgen_externrefs: WebAssembly.Table;
    readonly __wbindgen_free: (a: number, b: number, c: number) => void;
    readonly __wbindgen_exn_store: (a: number) => void;
    readonly __wbindgen_start: () => void;
}

export type SyncInitInput = BufferSource | WebAssembly.Module;

/**
 * Instantiates the given `module`, which can either be bytes or
 * a precompiled `WebAssembly.Module`.
 *
 * @param {{ module: SyncInitInput }} module - Passing `SyncInitInput` directly is deprecated.
 *
 * @returns {InitOutput}
 */
export function initSync(module: { module: SyncInitInput } | SyncInitInput): InitOutput;

/**
 * If `module_or_path` is {RequestInfo} or {URL}, makes a request and
 * for everything else, calls `WebAssembly.instantiate` directly.
 *
 * @param {{ module_or_path: InitInput | Promise<InitInput> }} module_or_path - Passing `InitInput` directly is deprecated.
 *
 * @returns {Promise<InitOutput>}
 */
export default function __wbg_init (module_or_path?: { module_or_path: InitInput | Promise<InitInput> } | InitInput | Promise<InitInput>): Promise<InitOutput>;
