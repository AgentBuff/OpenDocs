import type { MindmapModel } from "@open-office/schema/artifact";
import { findFontAsset, registerFont } from "../typography/font-loading.js";

/** Browser font metrics are view input to the canonical layout, never persisted node state. */
export async function measureMindmapText(model: MindmapModel): Promise<Record<string, { width: number; height: number }>> {
  // Creating thousands of hidden DOM nodes defeats viewport virtualization.
  // Large maps use the canonical engine's deterministic content heuristic;
  // precise browser metrics remain an optional small-map renderer input.
  if (model.nodes.length > 600) return {};
  const host = document.createElement("div");
  host.className = "mindmap-studio";
  host.setAttribute("aria-hidden", "true");
  host.style.cssText = "position:fixed;left:-100000px;top:0;visibility:hidden;pointer-events:none;width:max-content;height:auto;overflow:visible";
  const items = model.nodes.map(node => {
    const element = document.createElement("div");
    const level = node.parentId === null ? "root" : node.parentId === model.root ? "branch" : "leaf";
    element.className = `mindmap-node mindmap-node--${node.style.shape} mindmap-node--${level}`;
    element.style.cssText = "position:relative;width:max-content;height:auto;flex:none";
    element.style.borderWidth = `${node.style.borderWidth}px`;
    const text = document.createElement("span");
    text.className = "mindmap-node__text";
    const content = node.content;
    const characters = [...(content?.text || "未命名主题")];
    if (!content?.runs.length) text.textContent = characters.join("");
    else for (const run of content.runs) {
      const span = document.createElement("span");
      span.textContent = characters.slice(run.start, run.end).join("");
      span.style.fontFamily = run.style.fontFamily ?? "";
      span.style.fontSize = run.style.fontSize ? `${run.style.fontSize}px` : "";
      span.style.fontWeight = run.style.bold ? "700" : "";
      span.style.fontStyle = run.style.italic ? "italic" : "";
      text.append(span);
    }
    if (node.supplement.image) {
      // Reserve the renderer's maximum image width while measuring wrapped text.
      const image = document.createElement("span");
      image.style.cssText = `display:block;flex:none;width:45%;height:${Math.min(84, node.supplement.image.height ?? 84)}px;margin-right:.45rem`;
      element.append(image);
    }
    element.append(text);
    host.append(element);
    return { node, element, text };
  });
  document.body.append(host);
  try {
    const fonts = new Map<string, Promise<void>>();
    for (const { text } of items) for (const span of text.querySelectorAll<HTMLElement>("span")) {
      const asset = findFontAsset(span.style.fontFamily);
      if (asset && !fonts.has(asset.id)) fonts.set(asset.id, registerFont(asset));
    }
    await Promise.allSettled(fonts.values());
    await Promise.allSettled(items.flatMap(({ text }) => Array.from(text.querySelectorAll<HTMLElement>("span")).map(span => {
      const style = getComputedStyle(span);
      return document.fonts.load(`${style.fontStyle} ${style.fontWeight} ${style.fontSize} ${style.fontFamily}`, span.textContent ?? "");
    })));
    // Read natural widths together, then apply wrapping constraints before reading heights.
    const widths = items.map(({ node, element }) => Math.min(node.style.maxWidth, Math.max(node.style.minWidth, 160, Math.ceil(element.getBoundingClientRect().width))));
    items.forEach(({ element }, index) => { element.style.width = `${widths[index]}px`; });
    return Object.fromEntries(items.map(({ node, element }, index) => [node.id, {
      width: widths[index]!,
      height: Math.max(40, Math.ceil(element.getBoundingClientRect().height)) + (node.supplement.note || node.supplement.hyperlink ? 12 : 0),
    }]));
  } finally { host.remove(); }
}
