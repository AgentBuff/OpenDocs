import { FONT_ASSETS, type FontAsset } from "./fonts.js";
export { FONT_ASSETS, type FontAsset } from "./fonts.js";
const stylesheets = new Map<string, Promise<void>>();

export function findFontAsset(value: string): FontAsset | undefined {
  const primary = value.split(",")[0]?.trim().replace(/^['"]|['"]$/g, "").toLowerCase();
  return FONT_ASSETS.find(asset => asset.family.toLowerCase() === primary);
}

/** Register only the selected family. The browser fetches matching Unicode subsets. */
export function registerFont(asset: FontAsset): Promise<void> {
  const existing = stylesheets.get(asset.id);
  if (existing) return existing;
  const promise = new Promise<void>((resolve, reject) => {
    const link = document.createElement("link");
    link.rel = "stylesheet";
    link.href = asset.css;
    link.dataset.officeFont = asset.id;
    link.onload = () => resolve();
    link.onerror = () => { link.remove(); stylesheets.delete(asset.id); reject(new Error(`无法加载字体：${asset.family}`)); };
    document.head.append(link);
  });
  stylesheets.set(asset.id, promise);
  return promise;
}

/** Unlike fonts.check(), this requires actual downloaded FontFace objects. */
export async function loadFont(value: string, sample = "BESbswy", weight = 400, italic = false): Promise<FontFace[]> {
  const asset = findFontAsset(value);
  if (!asset) throw new Error(`字体不在可加载目录中：${value}`);
  await registerFont(asset);
  try {
    const faces = await document.fonts.load(`${italic ? "italic " : ""}${weight} 16px "${asset.family}"`, sample);
    if (!faces.length || faces.some(face => face.status !== "loaded")) throw new Error(`字体未覆盖当前预览文字：${asset.family}`);
    return faces;
  } catch (reason) {
    document.querySelector(`link[data-office-font="${asset.id}"]`)?.remove();
    stylesheets.delete(asset.id);
    throw reason;
  }
}

/** Restore fonts used by DOM renderers, including imported/persisted CSS stacks. */
export function observeRenderedFonts(root: HTMLElement): () => void {
  let scheduled = false;
  let disposed = false;
  const pending = new Set<HTMLElement>([root]);
  const scan = () => {
    scheduled = false;
    if (disposed) return;
    const values = new Set<string>();
    for (const node of pending) {
      if (!node.isConnected) continue;
      if (node.style.fontFamily) values.add(node.style.fontFamily);
      for (const child of node.querySelectorAll<HTMLElement>('[style*="font-family"]')) values.add(child.style.fontFamily);
    }
    pending.clear();
    for (const value of values) for (const family of value.split(',')) {
      const asset = findFontAsset(family);
      if (asset) void registerFont(asset).catch(() => { /* The picker exposes retry; preserve imported text while offline. */ });
    }
  };
  const observer = new MutationObserver(records => {
    for (const record of records) {
      if (record.type === 'attributes' && record.target instanceof HTMLElement && record.target.style.fontFamily) pending.add(record.target);
      else if (record.type === 'childList') for (const node of record.addedNodes) if (node instanceof HTMLElement) pending.add(node);
    }
    if (scheduled || !pending.size) return;
    scheduled = true;
    requestAnimationFrame(scan);
  });
  observer.observe(root, { subtree: true, childList: true, attributes: true, attributeFilter: ['style'] });
  scan();
  return () => { disposed = true; observer.disconnect(); pending.clear(); };
}
