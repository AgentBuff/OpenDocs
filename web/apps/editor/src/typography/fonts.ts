/**
 * 字体目录（C2 字体扩展）。
 *
 * 对齐主流中文办公套件（腾讯文档/Univer）的常用字体面，分为「中文 / 西文 /
 * 等宽」三组，平铺为 ToolbarSelect options —— 每组以一个 disabled 组头分隔。
 *
 * ⚠️ 值语义约束：`value` 必须是完整的 CSS font-family 栈字符串。编辑器把
 * `InlineStyle.fontFamily` 直接写入 `element.style.fontFamily`，而
 * `richTextFromHtml` 又会原样读回它去和模型比对（`richTextEquals`）；只有
 * DOM 往返后的字符串与持久化值完全相等，才不会被判定为“变化”而反复重建。
 * 因此这里不能存一个“token”再渲染时映射成栈 —— 那样往返会产生差异。
 */

/**
 * 与 `@open-office/ui` 的 `ToolbarSelectOption` 结构兼容的本地类型。
 * 有意不 import UI 包：字体目录是纯数据模块，不应反向依赖组件层。
 */
export interface FontOption {
  value: string;
  label: string;
  disabled?: boolean;
}

/** 组头选项：用于在平铺列表中标记分组，不可选择。 */
function groupHeader(label: string, value: string): FontOption {
  return { value, label, disabled: true };
}

export const FONT_GROUP_CHINESE = "oo-font-group-chinese";
export const FONT_GROUP_LATIN = "oo-font-group-latin";
export const FONT_GROUP_MONO = "oo-font-group-mono";

const CHINESE: FontOption[] = [
  {
    value: '"Noto Sans SC", "Source Han Sans SC", "PingFang SC", "Microsoft YaHei", sans-serif',
    label: "思源黑体",
  },
  {
    value: '"Noto Serif SC", "Source Han Serif SC", "Songti SC", "SimSun", serif',
    label: "思源宋体",
  },
  { value: '"Microsoft YaHei", "PingFang SC", "Noto Sans SC", sans-serif', label: "微软雅黑" },
  { value: '"PingFang SC", "Microsoft YaHei", "Noto Sans SC", sans-serif', label: "苹方" },
  { value: '"SimSun", "Songti SC", "Noto Serif SC", serif', label: "宋体" },
  { value: '"SimHei", "Heiti SC", "Noto Sans SC", sans-serif', label: "黑体" },
  { value: '"KaiTi", "Kaiti SC", "Noto Serif SC", serif', label: "楷体" },
  { value: '"FangSong", "FangSong SC", "Noto Serif SC", serif', label: "仿宋" },
];

const LATIN: FontOption[] = [
  { value: 'Arial, "Helvetica Neue", Helvetica, sans-serif', label: "Arial" },
  { value: 'Calibri, "Segoe UI", Arial, sans-serif', label: "Calibri" },
  { value: 'Georgia, "Times New Roman", serif', label: "Georgia" },
  { value: '"Times New Roman", Times, serif', label: "Times New Roman" },
  { value: 'Verdana, Geneva, sans-serif', label: "Verdana" },
  { value: 'Tahoma, "Segoe UI", Verdana, sans-serif', label: "Tahoma" },
  { value: '"Trebuchet MS", "Segoe UI", sans-serif', label: "Trebuchet MS" },
  { value: 'Helvetica, Arial, sans-serif', label: "Helvetica" },
];

const MONO: FontOption[] = [
  { value: 'Consolas, "SF Mono", Menlo, monospace', label: "Consolas" },
  { value: 'Menlo, Consolas, "Courier New", monospace', label: "Menlo" },
  { value: '"Courier New", Courier, monospace', label: "Courier New" },
  { value: '"JetBrains Mono", "SF Mono", Menlo, monospace', label: "JetBrains Mono" },
];

/** 平铺后的完整字体选项：占位项 + 中文组 + 西文组 + 等宽组。 */
export const FONT_OPTIONS: readonly FontOption[] = [
  { value: "", label: "字体" },
  groupHeader("中文", FONT_GROUP_CHINESE),
  ...CHINESE,
  groupHeader("西文", FONT_GROUP_LATIN),
  ...LATIN,
  groupHeader("等宽", FONT_GROUP_MONO),
  ...MONO,
];
