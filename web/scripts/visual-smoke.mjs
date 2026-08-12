#!/usr/bin/env node

/*
 * CDP visual smoke and screenshot artifact. This intentionally does not compare browser pixels
 * in JavaScript (font rasterization differs by OS); CI stores the PNG and a geometry manifest,
 * while reviewers compare it with the approved baseline in the visual-review job.
 */
import { mkdir, writeFile } from "node:fs/promises";
import { dirname, resolve } from "node:path";

const cdpPort = process.env.CDP_PORT ?? "9222";
const editorUrl = process.env.OO_EDITOR_URL;
const output = resolve(process.env.OO_VISUAL_OUTPUT ?? "/tmp/open-office-editor.png");
if (!editorUrl) throw new Error("OO_EDITOR_URL 必须指向临时 fixture 文档");
if (typeof WebSocket === "undefined") throw new Error("请使用 Node 22+ 运行 CDP visual smoke");

class Cdp {
  constructor(url) {
    this.socket = new WebSocket(url);
    this.nextId = 1;
    this.pending = new Map();
    this.socket.addEventListener("message", (event) => {
      const message = JSON.parse(event.data);
      const pending = this.pending.get(message.id);
      if (!pending) return;
      this.pending.delete(message.id);
      if (message.error) pending.reject(new Error(message.error.message));
      else pending.resolve(message.result);
    });
  }
  async open() {
    await new Promise((resolveOpen, reject) => {
      this.socket.addEventListener("open", resolveOpen, { once: true });
      this.socket.addEventListener("error", reject, { once: true });
    });
  }
  call(method, params = {}) {
    const id = this.nextId++;
    return new Promise((resolveCall, reject) => {
      this.pending.set(id, { resolve: resolveCall, reject });
      this.socket.send(JSON.stringify({ id, method, params }));
    });
  }
  close() { this.socket.close(); }
}
const delay = (ms) => new Promise((resolveDelay) => setTimeout(resolveDelay, ms));
async function evaluate(cdp, expression) {
  const result = await cdp.call("Runtime.evaluate", { expression, returnByValue: true, awaitPromise: true });
  if (result.exceptionDetails) throw new Error(result.exceptionDetails.text ?? "浏览器执行失败");
  return result.result?.value;
}
async function waitFor(cdp, expression) {
  const deadline = Date.now() + 15_000;
  while (Date.now() < deadline) {
    if (await evaluate(cdp, expression)) return;
    await delay(100);
  }
  throw new Error(`等待浏览器条件超时：${expression}`);
}

const targets = await fetch(`http://127.0.0.1:${cdpPort}/json/list`).then((response) => response.json());
const target = targets.find((item) => item.type === "page" && item.webSocketDebuggerUrl && item.url?.includes(":5174"))
  ?? targets.find((item) => item.type === "page" && item.webSocketDebuggerUrl);
if (!target) throw new Error(`没有找到 CDP page target，请确认端口 ${cdpPort} 已开启`);
const cdp = new Cdp(target.webSocketDebuggerUrl);
await cdp.open();
try {
  await cdp.call("Runtime.enable");
  await cdp.call("Page.enable");
  await cdp.call("Page.navigate", { url: editorUrl });
  await waitFor(cdp, "document.querySelector('[data-block-id], [data-table-grid], [data-artifact-kind]')");
  const manifest = await evaluate(cdp, `(() => {
    const root = document.documentElement;
    return {
      url: location.href,
      viewport: { width: innerWidth, height: innerHeight, dpr: devicePixelRatio },
      blocks: document.querySelectorAll('[data-block-id]').length,
      tables: document.querySelectorAll('[data-table-grid]').length,
      horizontalOverflow: root.scrollWidth > root.clientWidth + 1,
      verticalOverflow: root.scrollHeight > root.clientHeight + 1,
      toolbar: Boolean(document.querySelector('[data-toolbar-root], [role=toolbar]')),
    };
  })()`);
  if (manifest.horizontalOverflow) throw new Error("视觉 smoke 发现全局横向溢出");
  const screenshot = await cdp.call("Page.captureScreenshot", { format: "png", captureBeyondViewport: false });
  await mkdir(dirname(output), { recursive: true });
  await writeFile(output, Buffer.from(screenshot.data, "base64"));
  const manifestPath = `${output}.json`;
  await writeFile(manifestPath, `${JSON.stringify(manifest, null, 2)}\n`);
  console.log(JSON.stringify({ output, manifestPath, ...manifest }, null, 2));
} finally {
  cdp.close();
}
