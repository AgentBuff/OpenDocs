import type { MindmapModel } from "@open-office/schema/artifact";

export const MINDMAP_THEMES = [
  { id: "ocean", name: "清新蓝", root: "#3265d9", branches: ["#4475cf", "#24928c", "#9b68c2", "#ca8541"] },
  { id: "forest", name: "青竹绿", root: "#28765d", branches: ["#388367", "#72934d", "#428995", "#a28b47"] },
  { id: "sunset", name: "暖杏橙", root: "#b96530", branches: ["#bd783f", "#b35f75", "#8a75b2", "#7f9152"] },
  { id: "violet", name: "暮光紫", root: "#7554ad", branches: ["#8463ba", "#567fb5", "#b45f96", "#4e928e"] },
  { id: "graphite", name: "极简灰", root: "#424a58", branches: ["#626b7a", "#626b7a", "#626b7a", "#626b7a"] },
  { id: "rainbow", name: "多彩灵感", root: "#39465b", branches: ["#397cd0", "#d17b32", "#348d70", "#af5c9a"] },
] as const;

export function mindmapTheme(id: string | null | undefined) {
  return MINDMAP_THEMES.find((preset) => preset.id === id) ?? MINDMAP_THEMES[0];
}

/** Palette inheritance is a view projection; explicit node styles remain authoritative. */
export function mindmapBranchColors(model: MindmapModel | null) {
  const colors = new Map<string, string>();
  if (!model) return colors;
  const preset = mindmapTheme(model.settings.themeId);
  const children = new Map<string, string[]>();
  for (const node of model.nodes) {
    if (!node.parentId) continue;
    const siblings = children.get(node.parentId) ?? [];
    siblings.push(node.id);
    children.set(node.parentId, siblings);
  }
  if (!model.root) return colors;
  colors.set(model.root, preset.root);
  const queue = (children.get(model.root) ?? []).map((id, index) => ({ id, color: preset.branches[index % preset.branches.length] }));
  for (let at = 0; at < queue.length; at++) {
    const { id, color } = queue[at];
    colors.set(id, color);
    for (const child of children.get(id) ?? []) queue.push({ id: child, color });
  }
  return colors;
}

export function nodeThemeColors(depth: number, accent: string, mode: "light" | "dark" | "highContrast") {
  const surface = mode === "dark" ? "#22262e" : "#ffffff";
  const text = mode === "dark" ? "#e3e7ef" : "#303640";
  if (mode === "highContrast") return { fillColor: depth === 0 ? "#111111" : surface, borderColor: "#555555", textColor: depth === 0 ? "#ffffff" : "#111111" };
  const mix = (weight: number) => "#" + [1, 3, 5].map((at) => Math.round(parseInt(accent.slice(at, at + 2), 16) * weight + parseInt(surface.slice(at, at + 2), 16) * (1 - weight)).toString(16).padStart(2, "0")).join("");
  return { fillColor: depth === 0 ? accent : depth === 1 ? mix(.1) : surface, borderColor: depth === 0 ? accent : mix(depth === 1 ? .38 : .28), textColor: depth === 0 ? "#ffffff" : text };
}
