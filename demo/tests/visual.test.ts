/**
 * Visual comparison test: AGG software render vs WebGL render.
 *
 * Renders the same UI via both paths at a fixed size, saves PNGs,
 * and asserts they are nearly identical pixel-by-pixel.
 *
 * Run:  cd demo && bunx playwright test
 *       (starts server automatically via playwright.config.ts webServer)
 */

import { test, expect } from "@playwright/test";
import * as fs from "fs";
import * as path from "path";

const SNAPSHOT_DIR = path.join(__dirname, "snapshots");
const WIDTH  = 1200;
const HEIGHT = 800;

// Maximum allowed fraction of pixels that may differ (0–1).
// Very lenient for the first iteration — tighten once GL matches AGG closely.
const MAX_DIFF_FRACTION = 0.20;
// Per-channel tolerance for "same" pixel (0–255).
const PER_CHANNEL_TOLERANCE = 15;

/** Write RGBA bytes (Y-down) to a PNG file via an offscreen canvas. */
async function writePng(
  page: import("@playwright/test").Page,
  dataUrl: string,
  filename: string,
): Promise<void> {
  // Playwright can save dataUrl screenshots directly.
  const base64 = dataUrl.replace(/^data:image\/png;base64,/, "");
  fs.writeFileSync(path.join(SNAPSHOT_DIR, filename), Buffer.from(base64, "base64"));
}

test("AGG software render matches WebGL render", async ({ page }) => {
  // Navigate and wait for WASM to load (the loading overlay gets display:none).
  await page.goto("/");
  await page.locator("#loading").waitFor({ state: "hidden", timeout: 20_000 });

  // Give the animation loop one frame to fully render.
  await page.waitForTimeout(200);

  // ── 1. Capture WebGL render ──────────────────────────────────────────────
  // The GL canvas may not have preserveDrawingBuffer, so we call render()
  // and capture toDataURL() in the same synchronous JS task to beat the
  // browser's compositor clear.
  const glDataUrl = await page.evaluate(
    ([w, h]: [number, number]) => {
      const wasm = (window as unknown as Record<string, unknown>).__wasm as Record<string, (...args: unknown[]) => unknown>;
      const canvas = document.getElementById("canvas") as HTMLCanvasElement;
      // Force the canvas to our test size.
      canvas.width  = w;
      canvas.height = h;
      // Render (writes into the GL framebuffer).
      wasm["render"](w, h);
      // Capture before the browser composites and clears.
      return canvas.toDataURL("image/png");
    },
    [WIDTH, HEIGHT] as [number, number],
  );

  // ── 2. Capture AGG software render ──────────────────────────────────────
  // render_software_pixels() returns raw RGBA bytes (Y-down).
  // We draw them into an offscreen canvas and export as PNG.
  const softwareDataUrl = await page.evaluate(
    ([w, h]: [number, number]) => {
      const wasm = (window as unknown as Record<string, unknown>).__wasm as Record<string, (...args: unknown[]) => unknown>;
      const rawBytes = wasm["render_software_pixels"](w, h) as Uint8Array;

      const offscreen = document.createElement("canvas");
      offscreen.width  = w;
      offscreen.height = h;
      const ctx2d = offscreen.getContext("2d")!;
      const imageData = new ImageData(new Uint8ClampedArray(rawBytes.buffer), w, h);
      ctx2d.putImageData(imageData, 0, 0);
      return offscreen.toDataURL("image/png");
    },
    [WIDTH, HEIGHT] as [number, number],
  );

  // ── 3. Save both images ──────────────────────────────────────────────────
  fs.mkdirSync(SNAPSHOT_DIR, { recursive: true });
  await writePng(page, glDataUrl,       "gl_render.png");
  await writePng(page, softwareDataUrl, "software_render.png");

  console.log(`Saved snapshots to ${SNAPSHOT_DIR}`);

  // ── 4. Pixel-by-pixel comparison ─────────────────────────────────────────
  // Decode both PNGs into raw pixels using an offscreen canvas, then compare.
  const diffResult = await page.evaluate(
    async ([glUrl, swUrl, tol]: [string, string, number]) => {
      async function loadPixels(dataUrl: string): Promise<Uint8ClampedArray> {
        return new Promise((resolve) => {
          const img = new Image();
          img.onload = () => {
            const c = document.createElement("canvas");
            c.width  = img.width;
            c.height = img.height;
            c.getContext("2d")!.drawImage(img, 0, 0);
            resolve(c.getContext("2d")!.getImageData(0, 0, img.width, img.height).data);
          };
          img.src = dataUrl;
        });
      }

      const glPx = await loadPixels(glUrl);
      const swPx = await loadPixels(swUrl);

      if (glPx.length !== swPx.length) {
        return { error: `length mismatch: gl=${glPx.length} sw=${swPx.length}` };
      }

      let diffPixels = 0;
      const totalPixels = glPx.length / 4;
      for (let i = 0; i < glPx.length; i += 4) {
        const dr = Math.abs(glPx[i]   - swPx[i]);
        const dg = Math.abs(glPx[i+1] - swPx[i+1]);
        const db = Math.abs(glPx[i+2] - swPx[i+2]);
        // Ignore alpha channel differences (GL background vs AGG background may differ).
        if (dr > tol || dg > tol || db > tol) {
          diffPixels++;
        }
      }

      return { diffPixels, totalPixels, diffFraction: diffPixels / totalPixels };
    },
    [glDataUrl, softwareDataUrl, PER_CHANNEL_TOLERANCE] as [string, string, number],
  );

  if ("error" in diffResult) {
    throw new Error(`Comparison failed: ${diffResult.error}`);
  }

  const { diffPixels, totalPixels, diffFraction } = diffResult as {
    diffPixels: number;
    totalPixels: number;
    diffFraction: number;
  };

  console.log(
    `Pixel diff: ${diffPixels}/${totalPixels} (${(diffFraction * 100).toFixed(1)}%)` +
    `  tolerance=${PER_CHANNEL_TOLERANCE}/channel  limit=${(MAX_DIFF_FRACTION * 100).toFixed(0)}%`,
  );

  // The assertion — tighten MAX_DIFF_FRACTION as rendering improves.
  expect(diffFraction).toBeLessThanOrEqual(MAX_DIFF_FRACTION);
});
