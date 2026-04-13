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
 * Render the same app via the AGG software path and return raw RGBA pixels.
 *
 * The framebuffer is Y-up (row 0 = bottom).  For HTML Canvas `putImageData`
 * (which is Y-down), flip the rows in JS or use `pixels_flipped`.
 * Returns a byte array of length `width * height * 4` (RGBA, 8-bit per channel).
 */
export function render_software_pixels(width: number, height: number): Uint8Array;

/**
 * Render "TESTING FONT RENDERING" via the GL/tess2 path and return raw RGBA
 * pixels (Y-down, same format as `render_text_software`).
 *
 * Uses `gl.readPixels` to capture the result within the same task (before the
 * browser compositor clears the framebuffer).  Does NOT resize the canvas, so
 * the WebGL context remains valid across calls.  The render is always done into
 * a `width × height` region anchored at the bottom-left of the canvas.
 */
export function render_text_gl_pixels(width: number, height: number): Uint8Array;

/**
 * Render "TESTING FONT RENDERING" via the AGG software path.
 * Returns Y-down RGBA bytes (ready for `putImageData`).
 */
export function render_text_software(width: number, height: number): Uint8Array;

/**
 * Initialise panic hook so Rust panics appear in the browser console.
 */
export function wasm_start(): void;

export type InitInput = RequestInfo | URL | Response | BufferSource | WebAssembly.Module;

export interface InitOutput {
    readonly memory: WebAssembly.Memory;
    readonly render: (a: number, b: number) => void;
    readonly render_software_pixels: (a: number, b: number) => [number, number];
    readonly render_text_software: (a: number, b: number) => [number, number];
    readonly render_text_gl_pixels: (a: number, b: number) => [number, number];
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
