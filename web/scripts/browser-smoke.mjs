/*
 * Chromium/CDP 编辑器冒烟回归。
 *
 * 运行前用 --remote-debugging-port 启动已登录浏览器，并设置
 * OO_EDITOR_URL=http://127.0.0.1:5174/?doc=<可写测试文档>。
 * 该脚本只依赖 Node 22 的 WebSocket，不把浏览器状态伪造进 Vitest。
 */

const cdpPort = process.env.CDP_PORT ?? "9222";
const editorUrl = process.env.OO_EDITOR_URL;

if (!editorUrl) {
  throw new Error("OO_EDITOR_URL 必须指向可写的临时文档，避免冒烟脚本污染工作文档");
}

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
    await new Promise((resolve, reject) => {
      this.socket.addEventListener("open", resolve, { once: true });
      this.socket.addEventListener("error", reject, { once: true });
    });
  }

  call(method, params = {}) {
    const id = this.nextId++;
    return new Promise((resolve, reject) => {
      this.pending.set(id, { resolve, reject });
      this.socket.send(JSON.stringify({ id, method, params }));
    });
  }

  close() {
    this.socket.close();
  }
}

const delay = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

async function evaluate(cdp, expression) {
  const result = await cdp.call("Runtime.evaluate", {
    expression,
    awaitPromise: true,
    returnByValue: true,
  });
  if (result.exceptionDetails) throw new Error(result.exceptionDetails.text ?? "浏览器执行失败");
  return result.result?.value;
}

async function waitFor(cdp, expression, timeoutMs = 8000) {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    if (await evaluate(cdp, expression)) return;
    await delay(100);
  }
  throw new Error(`等待浏览器条件超时：${expression}`);
}

async function main() {
  if (typeof WebSocket === "undefined") {
    throw new Error("当前 Node 没有 WebSocket；请使用 Node 22+ 运行 CDP 冒烟脚本");
  }
  const targets = await fetch(`http://127.0.0.1:${cdpPort}/json/list`).then((response) => response.json());
  const editorOrigin = new URL(editorUrl).origin;
  const target = targets.find(
    (item) =>
      item.type === "page" &&
      item.webSocketDebuggerUrl &&
      (item.url?.startsWith(editorOrigin) || item.url?.includes(":5174")),
  ) ?? targets.find((item) => item.type === "page" && item.webSocketDebuggerUrl);
  if (!target) throw new Error(`没有找到 CDP page target，请确认端口 ${cdpPort} 已开启`);

  const cdp = new Cdp(target.webSocketDebuggerUrl);
  await cdp.open();
  try {
    await cdp.call("Runtime.enable");
    await cdp.call("Page.enable");
    await cdp.call("Page.navigate", { url: editorUrl });
    await waitFor(cdp, "document.querySelector('[contenteditable=\"true\"]')");

    const initialBlocks = await evaluate(cdp, "document.querySelectorAll('[data-block-id]').length");
    await evaluate(cdp, "document.querySelector('[contenteditable=\"true\"]').focus()");
    await cdp.call("Input.insertText", { text: " CDP smoke" });
    await cdp.call("Input.dispatchKeyEvent", { type: "keyDown", key: "Enter", code: "Enter", windowsVirtualKeyCode: 13 });
    await cdp.call("Input.dispatchKeyEvent", { type: "keyUp", key: "Enter", code: "Enter", windowsVirtualKeyCode: 13 });
    await waitFor(cdp, `document.querySelectorAll('[data-block-id]').length > ${initialBlocks}`);

    await evaluate(cdp, "document.querySelector('.block-row__handle').click()");
    await waitFor(cdp, "document.querySelector('[role=\"menu\"]')");
    await evaluate(cdp, "[...document.querySelectorAll('[role=\"menuitem\"]')].find((item) => item.textContent.includes('在下方插入'))?.click()");
    await waitFor(cdp, `document.querySelectorAll('[data-block-id]').length > ${initialBlocks + 1}`);

    await cdp.call("Page.reload", { ignoreCache: true });
    await waitFor(cdp, "document.querySelector('[contenteditable=\"true\"]')");
    const finalBlocks = await evaluate(cdp, "document.querySelectorAll('[data-block-id]').length");
    if (finalBlocks < initialBlocks + 1) throw new Error("刷新后 block 数量没有恢复");
    console.log(`CDP smoke passed: ${initialBlocks} -> ${finalBlocks} blocks`);
  } finally {
    cdp.close();
  }
}

main().catch((error) => {
  console.error(error instanceof Error ? error.message : error);
  process.exitCode = 1;
});
