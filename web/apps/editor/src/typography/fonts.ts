/** One asset-backed font catalog for every artifact editor. */
import assets from "./font-assets.json";
export type FontAsset = typeof assets[number];
export const FONT_ASSETS: readonly FontAsset[] = assets;
export interface FontOption { value: string; label: string; disabled?: boolean }
export const FONT_GROUP_CHINESE = "oo-font-group-chinese";
export const FONT_GROUP_LATIN = "oo-font-group-latin";
export const FONT_GROUP_MONO = "oo-font-group-mono";
export const FONT_LABELS: Record<string, string> = {
  'noto-sans-sc': '思源黑体', 'noto-serif-sc': '思源宋体',
  'noto-sans-tc': '思源黑体（繁体）', 'noto-serif-tc': '思源宋体（繁体）',
  'zcool-xiaowei': '站酷小薇体', 'zcool-qingke-huangyou': '站酷庆科黄油体', 'zcool-kuaile': '站酷快乐体',
  'ma-shan-zheng': '马善政楷书', 'long-cang': '龙藏体', 'zhi-mang-xing': '志莽行书',
  'liu-jian-mao-cao': '刘建毛草', 'lxgw-wenkai-tc': '霞鹜文楷（繁体）',
};
export function fontValue(font: FontAsset): string {
  const family = font.family.includes(' ') ? `"${font.family}"` : font.family;
  const generic = font.category === 'monospace' ? 'monospace' : font.category === 'serif' ? 'serif' : 'sans-serif';
  return `${family}, ${generic}`;
}

const chinese = (font: FontAsset) => font.subsets.some(subset => subset.includes("chinese") || subset === "japanese" || subset === "korean");
const option = (font: FontAsset): FontOption => ({ value: fontValue(font), label: FONT_LABELS[font.id] ?? font.family });
export const FONT_OPTIONS: readonly FontOption[] = [
  { value: "", label: "字体" },
  { value: FONT_GROUP_CHINESE, label: "中文与东亚文字", disabled: true },
  ...FONT_ASSETS.filter(chinese).map(option),
  { value: FONT_GROUP_LATIN, label: "西文与其他文字", disabled: true },
  ...FONT_ASSETS.filter(font => !chinese(font) && font.category !== "monospace").map(option),
  { value: FONT_GROUP_MONO, label: "等宽", disabled: true },
  ...FONT_ASSETS.filter(font => !chinese(font) && font.category === "monospace").map(option),
];
