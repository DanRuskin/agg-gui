/**
 * Text rendering isolation test.
 *
 * Renders "TESTING FONT RENDERING" via three independent paths at the same size:
 *   1. AGG software path     (Framebuffer + GfxCtx       → ground truth)
 *   2. tess2 + AGG path      (tess2 triangles → GfxCtx   → proves tess2 geometry)
 *   3. GL / tess2 path       (tess2 triangles → WebGL2   → tests full GL pipeline)
 *
 * All three images are saved as PNGs in tests/snapshots/.
 *
 * Comparisons:
 *   software ≈ tess_agg  → tess2 geometry is correct
 *   tess_agg ≈ gl        → GL pipeline is correct
 *
 * Run:  cd demo && bunx playwright test text_rendering
 */

import { test, expect } from "@playwright/test";
import * as fs from "fs";
import * as path from "path";

const SNAPSHOT_DIR = path.join(__dirname, "snapshots");
const WIDTH  = 600;
const HEIGHT = 80;

// Per-channel tolerance (0–255) for "same" pixel.
const TOLERANCE = 20;
// Maximum allowed fraction of pixels that may differ.
const MAX_DIFF_FRACTION = 0.15;

/** Decode a data URL to RGBA pixels in the browser. */
function jsLoadPixels(dataUrl: string): Promise<Uint8ClampedArray> {
  return new Promise((resolve) => {
    const img = new Image();
    img.onload = () => {
      const c = document.createElement("canvas");
      c.width = img.width; c.height = img.height;
      c.getContext("2d")!.drawImage(img, 0, 0);
      resolve(c.getContext("2d")!.getImageData(0, 0, img.width, img.height).data);
    };
    img.src = dataUrl;
  });
}

/** Compare two data URLs pixel-by-pixel and return diff stats. */
async function compareDataUrls(
  page: import("@playwright/test").Page,
  urlA: string,
  urlB: string,
  tol: number,
): Promise<{ diffPixels: number; totalPixels: number; diffFraction: number }> {
  return page.evaluate(
    async ([a, b, t]: [string, string, number]) => {
      const load = (url: string): Promise<Uint8ClampedArray> =>
        new Promise((resolve) => {
          const img = new Image();
          img.onload = () => {
            const c = document.createElement("canvas");
            c.width = img.width; c.height = img.height;
            c.getContext("2d")!.drawImage(img, 0, 0);
            resolve(c.getContext("2d")!.getImageData(0, 0, img.width, img.height).data);
          };
          img.src = url;
        });
      const pa = await load(a);
      const pb = await load(b);
      let diff = 0;
      const total = pa.length / 4;
      for (let i = 0; i < pa.length; i += 4) {
        if (Math.abs(pa[i]   - pb[i])   > t ||
            Math.abs(pa[i+1] - pb[i+1]) > t ||
            Math.abs(pa[i+2] - pb[i+2]) > t) diff++;
      }
      return { diffPixels: diff, totalPixels: total, diffFraction: diff / total };
    },
    [urlA, urlB, tol] as [string, string, number],
  );
}

/** Render a data URL into a PNG file. */
function saveDataUrl(dataUrl: string, name: string): void {
  fs.writeFileSync(
    path.join(SNAPSHOT_DIR, name),
    Buffer.from(dataUrl.replace(/^data:image\/png;base64,/, ""), "base64"),
  );
}

/** Call a WASM render function and return a data URL via an offscreen canvas. */
async function wasmRenderToDataUrl(
  page: import("@playwright/test").Page,
  fnName: string,
  w: number,
  h: number,
): Promise<string> {
  return page.evaluate(
    ([fn, width, height]: [string, number, number]) => {
      const wasm = (window as unknown as Record<string, unknown>).__wasm as Record<string, (...args: unknown[]) => unknown>;
      const rawBytes = wasm[fn](width, height) as Uint8Array;
      const offscreen = document.createElement("canvas");
      offscreen.width  = width;
      offscreen.height = height;
      const ctx2d = offscreen.getContext("2d")!;
      ctx2d.putImageData(new ImageData(new Uint8ClampedArray(rawBytes.buffer), width, height), 0, 0);
      return offscreen.toDataURL("image/png");
    },
    [fnName, w, h] as [string, number, number],
  );
}

test("text rendering: software path matches GL/tess2 path", async ({ page }) => {
  page.on("console", msg => { console.log(`[${msg.type()}] ${msg.text()}`); });
  page.on("pageerror", err => console.error("PAGE ERROR:", err.message));
  await page.goto("/");
  await page.locator("#loading").waitFor({ state: "hidden", timeout: 20_000 });

  // ── 1. Render all three paths ────────────────────────────────────────────────
  const softwareDataUrl = await wasmRenderToDataUrl(page, "render_text_software",   WIDTH, HEIGHT);
  const tessAggDataUrl  = await wasmRenderToDataUrl(page, "render_text_tess_agg_pixels", WIDTH, HEIGHT);
  const glDataUrl       = await wasmRenderToDataUrl(page, "render_text_gl_pixels",   WIDTH, HEIGHT);

  // ── 2. Save snapshots ────────────────────────────────────────────────────────
  fs.mkdirSync(SNAPSHOT_DIR, { recursive: true });
  saveDataUrl(softwareDataUrl, "text_software.png");
  saveDataUrl(tessAggDataUrl,  "text_tess_agg.png");
  saveDataUrl(glDataUrl,       "text_gl.png");
  console.log(`Snapshots saved → ${SNAPSHOT_DIR}`);

  // ── 3. Compare: software vs tess+AGG (tests tess2 geometry) ─────────────────
  const swVsTessAgg = await compareDataUrls(page, softwareDataUrl, tessAggDataUrl, TOLERANCE);
  console.log(
    `[software vs tess+AGG]  diff=${swVsTessAgg.diffPixels}/${swVsTessAgg.totalPixels}` +
    ` (${(swVsTessAgg.diffFraction * 100).toFixed(1)}%)`,
  );

  // ── 4. Compare: tess+AGG vs GL (tests GL pipeline) ──────────────────────────
  const tessAggVsGl = await compareDataUrls(page, tessAggDataUrl, glDataUrl, TOLERANCE);
  console.log(
    `[tess+AGG vs GL]        diff=${tessAggVsGl.diffPixels}/${tessAggVsGl.totalPixels}` +
    ` (${(tessAggVsGl.diffFraction * 100).toFixed(1)}%)`,
  );

  // ── 5. Also compare software vs GL directly for the assertion ───────────────
  const swVsGl = await compareDataUrls(page, softwareDataUrl, glDataUrl, TOLERANCE);
  console.log(
    `[software vs GL]        diff=${swVsGl.diffPixels}/${swVsGl.totalPixels}` +
    ` (${(swVsGl.diffFraction * 100).toFixed(1)}%)`,
    ` tol=${TOLERANCE}/ch, limit=${(MAX_DIFF_FRACTION * 100).toFixed(0)}%`,
  );

  // The primary assertion: end-to-end software vs GL must agree.
  expect(swVsGl.diffFraction, "software vs GL diff too high").toBeLessThanOrEqual(MAX_DIFF_FRACTION);
});
