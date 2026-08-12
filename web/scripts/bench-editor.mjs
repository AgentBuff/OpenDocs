/*
 * Editor performance baseline over Chromium CDP.
 *
 * Usage:
 *   OO_EDITOR_URL='http://127.0.0.1:5174/?doc=<writable-doc>' pnpm bench:editor
 *
 * The script deliberately measures the real editor surface instead of mounting a
 * synthetic React tree. It reports layout invariants, block count, input latency,
 * and long tasks. The test document is expected to be disposable because the
 * input probe inserts a short marker.
 */

const cdpPort = process.env.CDP_PORT ?? "9222";
const editorUrl = process.env.OO_EDITOR_URL;
const iterations = Number.parseInt(process.env.OO_BENCH_ITERATIONS ?? "12", 10);

if (!editorUrl) {
  throw new Error("OO_EDITOR_URL 必须指向可写的临时文档，避免性能探针污染工作文档");
}
if (!Number.isInteger(iterations) || iterations < 3 || iterations > 100) {
  throw new Error("OO_BENCH_ITERATIONS 必须是 3 到 100 之间的整数");
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

const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

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
    await sleep(100);
  }
  throw new Error(`等待浏览器条件超时：${expression}`);
}

async function performanceMetrics(cdp) {
  const result = await cdp.call("Performance.getMetrics");
  return Object.fromEntries(
    (result.metrics ?? []).map(({ name, value }) => [name, value]),
  );
}

function percentile(values, p) {
  const sorted = [...values].sort((a, b) => a - b);
  const index = Math.min(sorted.length - 1, Math.max(0, Math.ceil(sorted.length * p) - 1));
  return sorted[index] ?? 0;
}

async function main() {
  if (typeof WebSocket === "undefined") {
    throw new Error("当前 Node 没有 WebSocket；请使用 Node 22+ 运行性能基准");
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
    await cdp.call("Performance.enable");
    await cdp.call("Page.navigate", { url: editorUrl });
    await waitFor(cdp, "document.querySelector('[contenteditable=\"true\"]')");

    const baseline = await evaluate(cdp, `(() => ({
      blocks: document.querySelectorAll('[data-block-id]').length,
      scrollWidth: document.documentElement.scrollWidth,
      clientWidth: document.documentElement.clientWidth,
      viewportWidth: window.innerWidth,
      viewportHeight: window.innerHeight,
    }))()`);
    const baselineMetrics = await performanceMetrics(cdp);
    const baselineHeap = await cdp.call("Runtime.getHeapUsage");

    const observersInstalled = await evaluate(cdp, `(() => {
      window.__ooBenchLongTasks = [];
      window.__ooBenchFrameIntervals = [];
      window.__ooBenchDomMutations = 0;
      if (window.PerformanceObserver && PerformanceObserver.supportedEntryTypes.includes('longtask')) {
        const observer = new PerformanceObserver((list) => {
          for (const entry of list.getEntries()) window.__ooBenchLongTasks.push(entry.duration);
        });
        observer.observe({ type: 'longtask', buffered: false });
        window.__ooBenchLongTaskObserver = observer;
      }
      const mutationObserver = new MutationObserver((records) => {
        window.__ooBenchDomMutations += records.length;
      });
      mutationObserver.observe(document.body, { subtree: true, childList: true, attributes: true, characterData: true });
      window.__ooBenchMutationObserver = mutationObserver;
      let previousFrame;
      const frame = (timestamp) => {
        if (previousFrame !== undefined) window.__ooBenchFrameIntervals.push(timestamp - previousFrame);
        previousFrame = timestamp;
        window.__ooBenchFrameHandle = requestAnimationFrame(frame);
      };
      window.__ooBenchFrameHandle = requestAnimationFrame(frame);
      return true;
    })()`);
    if (!observersInstalled) throw new Error("无法安装性能观察器");

    const latencies = [];
    const editable = "document.querySelector('[contenteditable=\\\"true\\\"]')";
    for (let index = 0; index < iterations; index += 1) {
      const marker = `·${index}`;
      await evaluate(cdp, `${editable}.focus(); window.__ooBenchBefore = performance.now();`);
      await cdp.call("Input.insertText", { text: marker });
      await waitFor(cdp, `${editable}.textContent.includes(${JSON.stringify(marker)})`, 4000);
      const elapsed = await evaluate(cdp, "performance.now() - window.__ooBenchBefore");
      latencies.push(Number(elapsed));
    }
    await sleep(250);

    const report = await evaluate(cdp, `(() => {
      window.__ooBenchLongTaskObserver?.disconnect();
      window.__ooBenchMutationObserver?.disconnect();
      if (window.__ooBenchFrameHandle) cancelAnimationFrame(window.__ooBenchFrameHandle);
      const element = document.querySelector('[contenteditable=\"true\"]');
      return {
        blocks: document.querySelectorAll('[data-block-id]').length,
        scrollWidth: document.documentElement.scrollWidth,
        clientWidth: document.documentElement.clientWidth,
        textLength: element?.textContent?.length ?? 0,
        longTasks: window.__ooBenchLongTasks ?? [],
        frameIntervals: window.__ooBenchFrameIntervals ?? [],
        domMutations: window.__ooBenchDomMutations ?? 0,
        reactCommits: window.__ooBenchReactCommits ?? null,
      };
    })()`);
    const afterMetrics = await performanceMetrics(cdp);
    const afterHeap = await cdp.call("Runtime.getHeapUsage");

    const result = {
      url: editorUrl,
      iterations,
      baseline,
      afterInput: report,
      inputLatencyMs: {
        p50: percentile(latencies, 0.5),
        p95: percentile(latencies, 0.95),
        p99: percentile(latencies, 0.99),
        samples: latencies,
      },
      longTaskMs: {
        count: report.longTasks.length,
        max: report.longTasks.length ? Math.max(...report.longTasks) : 0,
        total: report.longTasks.reduce((sum, value) => sum + value, 0),
      },
      frameMs: {
        p95: percentile(report.frameIntervals, 0.95),
        max: report.frameIntervals.length ? Math.max(...report.frameIntervals) : 0,
        droppedOver16: report.frameIntervals.filter((value) => value > 16.7).length,
      },
      domMutations: report.domMutations,
      reactCommits: report.reactCommits,
      heapBytes: {
        usedBefore: baselineHeap.usedSize,
        usedAfter: afterHeap.usedSize,
        delta: afterHeap.usedSize - baselineHeap.usedSize,
      },
      chromiumMetrics: {
        layoutCountDelta: (afterMetrics.LayoutCount ?? 0) - (baselineMetrics.LayoutCount ?? 0),
        recalcStyleCountDelta: (afterMetrics.RecalcStyleCount ?? 0) - (baselineMetrics.RecalcStyleCount ?? 0),
        scriptDurationDeltaMs: ((afterMetrics.ScriptDuration ?? 0) - (baselineMetrics.ScriptDuration ?? 0)) * 1000,
        layoutDurationDeltaMs: ((afterMetrics.LayoutDuration ?? 0) - (baselineMetrics.LayoutDuration ?? 0)) * 1000,
        paintDurationDeltaMs: ((afterMetrics.PaintDuration ?? 0) - (baselineMetrics.PaintDuration ?? 0)) * 1000,
      },
      horizontalOverflow: report.scrollWidth > report.clientWidth,
    };
    console.log(JSON.stringify(result, null, 2));
    if (result.horizontalOverflow) throw new Error("编辑器存在横向溢出");
  } finally {
    cdp.close();
  }
}

main().catch((error) => {
  console.error(error instanceof Error ? error.message : error);
  process.exitCode = 1;
});
