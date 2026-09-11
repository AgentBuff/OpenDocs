import { expect, test } from "@playwright/test";
import { readFileSync } from "node:fs";
const assets = JSON.parse(readFileSync(new URL("../../src/typography/font-assets.json", import.meta.url), "utf8")) as Array<{ family: string; id: string; css: string; subsets: string[] }>;

// Validate actual FontFace loads, not CSS assignment or system fallback detection.
test("every supplied open font loads its own local face", async ({ page }) => {
  test.setTimeout(180_000);
  await page.goto("/");
  const failures = await page.evaluate(async catalog => {
    const failures: string[] = [];
    for (const asset of catalog) {
      try {
        await new Promise<void>((resolve, reject) => {
          const link = document.createElement("link");
          link.rel = "stylesheet"; link.href = asset.css;
          link.onload = () => resolve(); link.onerror = () => reject(new Error("stylesheet failed"));
          document.head.append(link);
        });
        const sample = asset.subsets.some(s => s.includes("chinese")) ? "字体中文" : asset.subsets.includes("japanese") ? "日本語" : asset.subsets.includes("korean") ? "한국어" : asset.subsets.includes("arabic") ? "العربية" : asset.subsets.includes("devanagari") ? "नमस्ते" : asset.subsets.includes("thai") ? "ภาษาไทย" : asset.subsets.includes("hebrew") ? "שלום" : "BESbswy";
        const faces = await document.fonts.load(`400 24px "${asset.family}"`, sample);
        if (!faces.length || faces.some(face => face.status !== "loaded" || face.family.replaceAll('"', '') !== asset.family)) throw new Error("matching font face not loaded");
      } catch (error) { failures.push(`${asset.id}: ${String(error)}`); }
    }
    return failures;
  }, assets);
  expect(failures).toEqual([]);
});

test("every shipped WOFF2 face decodes in the browser", async ({ page, request }) => {
  test.setTimeout(180_000);
  const response = await request.get('/fonts/manifest.json');
  expect(response.ok()).toBeTruthy();
  const manifest = await response.json() as Array<{ id: string; faces: Array<{ file: string }> }>;
  const urls = manifest.flatMap(font => font.faces.map(face => `/fonts/${font.id}/${face.file}`));
  expect(urls.length).toBeGreaterThan(assets.length);
  await page.goto('/');
  const failures = await page.evaluate(async urls => {
    const failures: string[] = [];
    let cursor = 0;
    await Promise.all(Array.from({ length: 12 }, async () => {
      while (cursor < urls.length) {
        const url = urls[cursor++]!;
        try { await new FontFace('Office font verification', `url("${url}")`).load(); }
        catch { failures.push(url); }
      }
    }));
    return failures;
  }, urls);
  expect(failures).toEqual([]);
});
