// agg-gui demo — Phase 7 frontend
//
// The full UI (tab bar + content) is rendered by agg-gui on the canvas.
// This file loads the WASM module and forwards browser events to it.

type RenderFn  = (width: number, height: number) => Uint8Array;
type MouseXYFn = (x: number, y: number) => void;
type MouseXYBFn = (x: number, y: number, button: number) => void;
type WheelFn   = (x: number, y: number, delta_y: number) => void;
type KeyFn     = (key: string, shift: boolean, ctrl: boolean, alt: boolean) => void;
type VoidFn    = () => void;

let wasmModule: Record<string, unknown> | null = null;

// --- Canvas setup ---

const canvas = document.getElementById("canvas") as HTMLCanvasElement;
const ctx2d = canvas.getContext("2d")!;
const loadingEl = document.getElementById("loading")!;
const statusEl = document.getElementById("status")!;

// --- Render loop ---

function render() {
  if (!wasmModule) return;

  const wrap = canvas.parentElement!;
  const dpr = window.devicePixelRatio || 1;
  const w = Math.floor(wrap.clientWidth * dpr);
  const h = Math.floor(wrap.clientHeight * dpr);

  if (canvas.width !== w || canvas.height !== h) {
    canvas.width = w;
    canvas.height = h;
  }
  if (w === 0 || h === 0) return;

  const t0 = performance.now();
  const pixels = (wasmModule["render"] as RenderFn)(w, h);
  const imageData = new ImageData(
    new Uint8ClampedArray(pixels.buffer, pixels.byteOffset, pixels.byteLength),
    w, h,
  );
  ctx2d.putImageData(imageData, 0, 0);

  const ms = (performance.now() - t0).toFixed(1);
  statusEl.textContent = `${w}×${h}  ${ms}ms`;
}

// --- Canvas coordinate helper ---

function canvasPos(e: MouseEvent): [number, number] {
  const rect = canvas.getBoundingClientRect();
  const dpr = window.devicePixelRatio || 1;
  return [(e.clientX - rect.left) * dpr, (e.clientY - rect.top) * dpr];
}

// --- Event forwarding ---

canvas.addEventListener("mousemove", (e) => {
  if (!wasmModule) return;
  const [x, y] = canvasPos(e);
  (wasmModule["on_mouse_move"] as MouseXYFn)(x, y);
  render();
});

canvas.addEventListener("mousedown", (e) => {
  if (!wasmModule) return;
  e.preventDefault();
  canvas.focus();
  const [x, y] = canvasPos(e);
  (wasmModule["on_mouse_down"] as MouseXYBFn)(x, y, e.button);
  render();
});

canvas.addEventListener("mouseup", (e) => {
  if (!wasmModule) return;
  const [x, y] = canvasPos(e);
  (wasmModule["on_mouse_up"] as MouseXYBFn)(x, y, e.button);
  render();
});

canvas.addEventListener("mouseleave", () => {
  if (!wasmModule) return;
  (wasmModule["on_mouse_leave"] as VoidFn)();
  render();
});

canvas.addEventListener("wheel", (e) => {
  if (!wasmModule) return;
  e.preventDefault();
  const [x, y] = canvasPos(e);
  const delta_y = e.deltaY / (e.deltaMode === 0 ? 40.0 : 1.0);
  (wasmModule["on_mouse_wheel"] as WheelFn)(x, y, delta_y);
  render();
}, { passive: false });

canvas.addEventListener("keydown", (e) => {
  if (!wasmModule) return;
  if (e.key !== "Tab") e.preventDefault();
  (wasmModule["on_key_down"] as KeyFn)(e.key, e.shiftKey, e.ctrlKey, e.altKey);
  render();
});

canvas.addEventListener("contextmenu", (e) => e.preventDefault());

// --- Resize observer ---

const ro = new ResizeObserver(() => render());
ro.observe(canvas.parentElement!);

// --- Load WASM ---

async function init() {
  try {
    const wasm = await import("../public/pkg/demo_wasm.js");
    const wasmUrl = new URL("./public/pkg/demo_wasm_bg.wasm", location.href);
    await wasm.default({ module_or_path: wasmUrl });

    wasmModule = wasm as unknown as Record<string, unknown>;
    loadingEl.classList.add("hidden");
    render();
  } catch (e) {
    loadingEl.textContent = `Error loading WASM: ${e}`;
    console.error(e);
  }
}

init();
