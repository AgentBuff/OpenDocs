/*
 * Chromium/CDP table interaction contract.
 *
 * Run with a writable test document that already contains a table:
 *   OO_EDITOR_URL='http://127.0.0.1:5174/?doc=<table-fixture>' pnpm browser:table-smoke
 *
 * This fixture only selects, hovers and opens menus. It intentionally does not
 * dispatch a mutating command, so it is safe to run against a shared preview
 * document when a disposable fixture is not available.
 */

const cdpPort = process.env.CDP_PORT ?? "9222";
const editorUrl = process.env.OO_EDITOR_URL;
if (!editorUrl) throw new Error("OO_EDITOR_URL 必须指向含表格的可写测试文档");
if (typeof WebSocket === "undefined") throw new Error("当前 Node 没有 WebSocket；请使用 Node 22+ 运行 CDP 冒烟脚本");

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

async function waitFor(cdp, expression, timeoutMs = 10000) {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    if (await evaluate(cdp, expression)) return;
    await delay(100);
  }
  throw new Error(`等待浏览器条件超时：${expression}`);
}

async function box(cdp, selector) {
  return evaluate(cdp, `(() => {
    const element = document.querySelector(${JSON.stringify(selector)});
    if (!element) return null;
    const rect = element.getBoundingClientRect();
    return { left: rect.left, top: rect.top, width: rect.width, height: rect.height };
  })()`);
}

async function moveMouse(cdp, rect) {
  if (!rect) throw new Error("无法取得交互目标的几何位置");
  await cdp.call("Input.dispatchMouseEvent", {
    type: "mouseMoved",
    x: rect.left + Math.max(1, rect.width / 2),
    y: rect.top + Math.max(1, rect.height / 2),
  });
}

async function rightClick(cdp, rect) {
  if (!rect) throw new Error("无法取得单元格几何位置");
  const x = rect.left + Math.max(1, rect.width / 2);
  const y = rect.top + Math.max(1, rect.height / 2);
  await cdp.call("Input.dispatchMouseEvent", { type: "mouseMoved", x, y });
  await cdp.call("Input.dispatchMouseEvent", { type: "mousePressed", button: "right", clickCount: 1, x, y });
  await cdp.call("Input.dispatchMouseEvent", { type: "mouseReleased", button: "right", clickCount: 1, x, y });
}

async function main() {
  const targets = await fetch(`http://127.0.0.1:${cdpPort}/json/list`).then((response) => response.json());
  const origin = new URL(editorUrl).origin;
  const target = targets.find((item) => item.type === "page" && item.webSocketDebuggerUrl && item.url?.startsWith(origin))
    ?? targets.find((item) => item.type === "page" && item.webSocketDebuggerUrl);
  if (!target) throw new Error(`没有找到 CDP page target，请确认端口 ${cdpPort} 已开启`);
  const cdp = new Cdp(target.webSocketDebuggerUrl);
  await cdp.open();
  try {
    await cdp.call("Runtime.enable");
    await cdp.call("Page.enable");
    await cdp.call("Page.navigate", { url: editorUrl });
    await waitFor(cdp, "document.querySelector('[data-table-grid=\\\"document\\\"]')");
    await waitFor(cdp, "document.querySelector('[data-table-selector=\\\"all\\\"]')");

    const tableCells = await evaluate(cdp, "document.querySelectorAll('[data-table-cell-id]').length");
    const rowSelectors = await evaluate(cdp, "document.querySelectorAll('[data-table-selector=\\\"row\\\"]').length");
    const columnSelectors = await evaluate(cdp, "document.querySelectorAll('[data-table-selector=\\\"column\\\"]').length");
    if (!tableCells || !rowSelectors || !columnSelectors) throw new Error("表格 fixture 缺少单元格或行列选择区");

    // The corner is a dedicated blank rounded hit region and selects the grid,
    // without letting row/column + affordances steal the shared intersection.
    await evaluate(cdp, "document.querySelector('[data-table-selector=\\\"all\\\"]').click()");
    await waitFor(cdp, "document.querySelector('[data-table-selector=\\\"all\\\"]').getAttribute('aria-pressed') === 'true'");
    const allSelected = await evaluate(cdp, "document.querySelectorAll('.block-table__cell--selected').length");
    if (allSelected <= 0) throw new Error("左上角全选没有填充选中单元格");

    await evaluate(cdp, "document.querySelector('[data-table-selector=\\\"row\\\"]').click()");
    await waitFor(cdp, "document.querySelector('[data-table-selector=\\\"row\\\"]').getAttribute('aria-pressed') === 'true'");
    await evaluate(cdp, "document.querySelector('[data-table-selector=\\\"column\\\"]').click()");
    await waitFor(cdp, "document.querySelector('[data-table-selector=\\\"column\\\"]').getAttribute('aria-pressed') === 'true'");

    // Boundary + controls are real hit targets (not a layout row). Hover each
    // direction and verify that its visual state is promoted above the grid.
    const rowPlus = await box(cdp, ".block-table__row-affordance");
    const columnPlus = await box(cdp, ".block-table__column-affordance");
    await moveMouse(cdp, rowPlus);
    await waitFor(cdp, "getComputedStyle(document.querySelector('.block-table__row-affordance')).opacity === '1'");
    await moveMouse(cdp, columnPlus);
    await waitFor(cdp, "getComputedStyle(document.querySelector('.block-table__column-affordance')).opacity === '1'");

    await rightClick(cdp, await box(cdp, "[data-table-cell-id]"));
    await waitFor(cdp, "document.querySelector('[data-table-menu=\\\"context\\\"]')");
    const menuZ = await evaluate(cdp, "Number.parseInt(getComputedStyle(document.querySelector('[data-table-menu=\\\"context\\\"]')).zIndex, 10)");
    if (!(menuZ >= 50)) throw new Error(`表格右键菜单层级异常：${menuZ}`);
    const insertAnchor = await box(cdp, ".block-table__context-submenu-anchor");
    await moveMouse(cdp, insertAnchor);
    await waitFor(cdp, "document.querySelector('[data-table-menu=\\\"submenu\\\"]')");
    const submenuGeometry = await box(cdp, "[data-table-menu=\\\"submenu\\\"]");
    if (!submenuGeometry || submenuGeometry.width <= 0 || submenuGeometry.height <= 0) throw new Error("右键级联菜单没有可见几何区域");

    await cdp.call("Emulation.setDeviceMetricsOverride", { width: 640, height: 800, deviceScaleFactor: 1, mobile: false });
    await delay(200);
    const narrow = await evaluate(cdp, `(() => {
      const wrap = document.querySelector('.block-table-wrap');
      return wrap && wrap.scrollWidth <= wrap.clientWidth + 1 && document.documentElement.scrollWidth <= window.innerWidth + 1;
    })()`);
    if (!narrow) throw new Error("窄屏表格产生了横向溢出或命中层撑宽布局");
    await cdp.call("Emulation.clearDeviceMetricsOverride");
    console.log(`CDP table smoke passed: cells=${tableCells}, rows=${rowSelectors}, columns=${columnSelectors}, allSelected=${allSelected}`);
  } finally {
    cdp.close();
  }
}

main().catch((error) => {
  console.error(error instanceof Error ? error.message : error);
  process.exitCode = 1;
});
