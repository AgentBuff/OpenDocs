/**
 * 内联 SVG 图标。
 *
 * 不引图标库：整个应用用到的图标不到二十个，内联既避免了一个几百 KB 的依赖，也让
 * 每个图标的尺寸和描边粗细可以和界面对齐。
 */

interface IconProps {
  className?: string;
}

type ArtifactIconKind = "document" | "spreadsheet" | "presentation" | "mindmap" | "whiteboard";

const base = {
  width: 16,
  height: 16,
  viewBox: "0 0 16 16",
  fill: "none",
  stroke: "currentColor",
  strokeWidth: 1.5,
  strokeLinecap: "round" as const,
  strokeLinejoin: "round" as const,
};

export const DocIcon = ({ className }: IconProps) => (
  <svg {...base} className={className} aria-hidden="true">
    <path d="M9 1.5H4a1 1 0 0 0-1 1v11a1 1 0 0 0 1 1h8a1 1 0 0 0 1-1V5.5L9 1.5Z" />
    <path d="M9 1.5v4h4" />
  </svg>
);

export const StarIcon = ({ filled, className }: IconProps & { filled: boolean }) => (
  <svg {...base} className={className} fill={filled ? "currentColor" : "none"} aria-hidden="true">
    <path d="M8 2.2l1.76 3.57 3.94.57-2.85 2.78.67 3.92L8 11.2l-3.52 1.85.67-3.92L2.3 6.34l3.94-.57L8 2.2Z" />
  </svg>
);

/** Theme switch affordance: sun in dark mode, moon in light mode. */
export const ThemeIcon = ({ dark, className }: IconProps & { dark: boolean }) => (
  <svg {...base} className={className} aria-hidden="true">
    {dark ? (
      <>
        <circle cx="8" cy="8" r="3.1" />
        <path d="M8 1.5v1.2M8 13.3v1.2M1.5 8h1.2M13.3 8h1.2M3.4 3.4l.9.9M11.7 11.7l.9.9M12.6 3.4l-.9.9M4.3 11.7l-.9.9" />
      </>
    ) : (
      <path d="M13.5 10.1A5.8 5.8 0 0 1 5.9 2.5 5.8 5.8 0 1 0 13.5 10.1Z" />
    )}
  </svg>
);

export const UploadIcon = ({ className }: IconProps) => (
  <svg {...base} className={className} aria-hidden="true">
    <path d="M8 10.5V2.5" />
    <path d="M4.5 6L8 2.5 11.5 6" />
    <path d="M2.5 11v2a1 1 0 0 0 1 1h9a1 1 0 0 0 1-1v-2" />
  </svg>
);

/** 首页新建面板与文件列表共用的资源类型图标。保持线性图标语义，颜色由容器决定。 */
export const ArtifactIcon = ({ kind, className }: IconProps & { kind: ArtifactIconKind }) => {
  const props = { ...base, className };
  switch (kind) {
    case "document":
      return (
        <svg {...props} aria-hidden="true">
          <path d="M5 2.5h4.3L12.5 5.7V13a1 1 0 0 1-1 1H5a1 1 0 0 1-1-1V3.5a1 1 0 0 1 1-1Z" />
          <path d="M9 2.5v3.2h3.2M6.2 8h3.9M6.2 10.3h3.9" />
        </svg>
      );
    case "spreadsheet":
      return (
        <svg {...props} aria-hidden="true">
          <rect x="2.5" y="2.5" width="11" height="11" rx="1" />
          <path d="M2.5 6.2h11M2.5 9.8h11M6.2 2.5v11M9.8 2.5v11" />
        </svg>
      );
    case "presentation":
      return (
        <svg {...props} aria-hidden="true">
          <rect x="2.5" y="3" width="11" height="8" rx="1" />
          <path d="M8 11v2.5M5.5 13.5h5M5.5 7.5l1.8-1.5 1.4 1.1 1.8-1.6" />
        </svg>
      );
    case "mindmap":
      return (
        <svg {...props} aria-hidden="true">
          <circle cx="8" cy="8" r="2" />
          <circle cx="3" cy="3.5" r="1.5" />
          <circle cx="13" cy="3.5" r="1.5" />
          <circle cx="13" cy="12.5" r="1.5" />
          <path d="m6.6 6.6-2.4-2M9.4 6.6l2.4-2M9.8 9.2l2.1 2" />
        </svg>
      );
    case "whiteboard":
      return (
        <svg {...props} aria-hidden="true">
          <rect x="2.5" y="2.5" width="11" height="11" rx="1.5" />
          <path d="m5 10 2-2 1.5 1.5L11 7M5 5.5h.1M8 5.5h.1" />
        </svg>
      );
  }
};

export const HomeIcon = ({ className }: IconProps) => (
  <svg {...base} className={className} aria-hidden="true">
    <path d="M2.5 7L8 2.5 13.5 7v6a1 1 0 0 1-1 1h-9a1 1 0 0 1-1-1V7Z" />
  </svg>
);

export const UndoIcon = ({ className }: IconProps) => (
  <svg {...base} className={className} aria-hidden="true">
    <path d="M3 8h7a3 3 0 0 1 0 6H7" />
    <path d="M5.5 5.5L3 8l2.5 2.5" />
  </svg>
);

export const RedoIcon = ({ className }: IconProps) => (
  <svg {...base} className={className} aria-hidden="true">
    <path d="M13 8H6a3 3 0 0 0 0 6h3" />
    <path d="M10.5 5.5L13 8l-2.5 2.5" />
  </svg>
);

/** 文档级功能菜单。作为产品入口使用，和具体编辑命令解耦。 */
export const MenuIcon = ({ className }: IconProps) => (
  <svg {...base} className={className} aria-hidden="true">
    <circle cx="8" cy="8" r="5.5" />
    <path d="M5.6 6.3h4.8M5.6 8h4.8M5.6 9.7h3.1" />
  </svg>
);

/** 插入内容入口。 */
export const InsertIcon = ({ className }: IconProps) => (
  <svg {...base} className={className} aria-hidden="true">
    <circle cx="8" cy="8" r="5.5" />
    <path d="M8 5v6M5 8h6" />
  </svg>
);

/** Block 行首的稳定操作把手：两列三行圆点，避免使用字符字形造成基线漂移。 */
export const BlockHandleIcon = ({ className }: IconProps) => (
  <svg {...base} className={className} fill="currentColor" stroke="none" aria-hidden="true">
    {[4, 8, 12].flatMap((y) => [5, 11].map((x) => <circle key={`${x}-${y}`} cx={x} cy={y} r="1.15" />))}
  </svg>
);

/** 对齐图标：四种对齐共用一套，靠 lines 描述每行的长度。 */
export const AlignIcon = ({
  align,
  className,
}: IconProps & { align: "left" | "center" | "right" | "justify" }) => {
  const rows = [0, 1, 2, 3].map((index) => {
    const full = { x1: 2.5, x2: 13.5 };
    if (align === "justify" || index % 2 === 0) return full;
    switch (align) {
      case "left":
        return { x1: 2.5, x2: 9.5 };
      case "right":
        return { x1: 6.5, x2: 13.5 };
      default:
        return { x1: 4.5, x2: 11.5 };
    }
  });
  return (
    <svg {...base} className={className} aria-hidden="true">
      {rows.map((row, index) => (
        <line key={index} x1={row.x1} x2={row.x2} y1={3.5 + index * 3} y2={3.5 + index * 3} />
      ))}
    </svg>
  );
};

export const IndentIcon = ({ increase, className }: IconProps & { increase: boolean }) => (
  <svg {...base} className={className} aria-hidden="true">
    <line x1="6.5" x2="13.5" y1="4" y2="4" />
    <line x1="6.5" x2="13.5" y1="8" y2="8" />
    <line x1="6.5" x2="13.5" y1="12" y2="12" />
    <path d={increase ? "M2.5 5.5L4.5 8l-2 2.5" : "M4.5 5.5L2.5 8l2 2.5"} />
  </svg>
);

export const LineHeightIcon = ({ className }: IconProps) => (
  <svg {...base} className={className} aria-hidden="true">
    <line x1="6.5" x2="13.5" y1="4" y2="4" />
    <line x1="6.5" x2="13.5" y1="8" y2="8" />
    <line x1="6.5" x2="13.5" y1="12" y2="12" />
    <path d="M3 5.5V10.5" />
    <path d="M1.8 6.8L3 5.5l1.2 1.3" />
    <path d="M1.8 9.2L3 10.5l1.2-1.3" />
  </svg>
);

export const SaveIcon = ({ className }: IconProps) => (
  <svg {...base} className={className} aria-hidden="true">
    <path d="M2.5 3.5a1 1 0 0 1 1-1h7L13.5 5.5v7a1 1 0 0 1-1 1h-9a1 1 0 0 1-1-1v-9Z" />
    <path d="M5 2.5v3h5" />
    <path d="M5 13.5v-4h6v4" />
  </svg>
);

export const HistoryIcon = ({ className }: IconProps) => (
  <svg {...base} className={className} aria-hidden="true">
    <circle cx="8" cy="8" r="5.5" />
    <path d="M8 4.5v3.8l2.4 1.4" />
    <path d="M2.5 5.5V3.2M2.5 3.2h2.3" />
  </svg>
);

export const DownloadIcon = ({ className }: IconProps) => (
  <svg {...base} className={className} aria-hidden="true">
    <path d="M8 2.5v7" />
    <path d="m5.2 7.2 2.8 2.8 2.8-2.8" />
    <path d="M3 11.5v1a1 1 0 0 0 1 1h8a1 1 0 0 0 1-1v-1" />
  </svg>
);

export const SettingsIcon = ({ className }: IconProps) => (
  <svg {...base} className={className} aria-hidden="true">
    <circle cx="8" cy="8" r="2.2" />
    <path d="m8 2.5.7 1.4 1.5.5 1.4-.6 1.1 1.1-.6 1.4.5 1.5 1.4.7v1.6l-1.4.7-.5 1.5.6 1.4-1.1 1.1-1.4-.6-1.5.5-.7 1.4H6.4l-.7-1.4-1.5-.5-1.4.6-1.1-1.1.6-1.4-.5-1.5-1.4-.7V8.4l1.4-.7.5-1.5-.6-1.4 1.1-1.1 1.4.6 1.5-.5.7-1.4H8Z" />
  </svg>
);

export const BrushIcon = ({ className }: IconProps) => (
  <svg {...base} className={className} aria-hidden="true">
    <path d="M10 2.5l3.5 3.5-5 5-3.5-3.5 5-5Z" />
    <path d="M5 7.5L2.5 10v3.5H6L8.5 11" />
  </svg>
);

export const ClearFormatIcon = ({ className }: IconProps) => (
  <svg {...base} className={className} aria-hidden="true">
    <path d="M6 3h7" />
    <path d="M9.5 3L7 13" />
    <path d="M2.5 9.5l4 4" />
    <path d="M6.5 9.5l-4 4" />
  </svg>
);

export const HighlightIcon = ({ className }: IconProps) => (
  <svg {...base} className={className} aria-hidden="true">
    <path d="M9.5 2.5l4 4-5.5 5.5H4.5v-3.5L9.5 2.5Z" />
  </svg>
);

export const ListIcon = ({ ordered, className }: IconProps & { ordered: boolean }) => (
  <svg {...base} className={className} aria-hidden="true">
    <line x1="6.5" x2="13.5" y1="4" y2="4" />
    <line x1="6.5" x2="13.5" y1="8" y2="8" />
    <line x1="6.5" x2="13.5" y1="12" y2="12" />
    {ordered ? (
      <>
        <path d="M2.4 2.8h1v2.4" strokeWidth="1.2" />
        <path d="M2.2 6.9h1.6l-1.6 2h1.6" strokeWidth="1.2" />
        <path d="M2.2 10.9h1.4v1.1H2.4v1.1h1.2" strokeWidth="1.2" />
      </>
    ) : (
      <>
        <circle cx="3" cy="4" r="1" fill="currentColor" stroke="none" />
        <circle cx="3" cy="8" r="1" fill="currentColor" stroke="none" />
        <circle cx="3" cy="12" r="1" fill="currentColor" stroke="none" />
      </>
    )}
  </svg>
);

/** Toolbar 的稳定图标语义。配置层只引用名称，不把 React 节点塞进能力注册表。 */
export type ToolbarIconName =
  | "undo"
  | "redo"
  | "insert"
  | "table"
  | "link"
  | "align-left"
  | "align-center"
  | "align-right"
  | "align-justify"
  | "bullet-list"
  | "ordered-list"
  | "indent-decrease"
  | "indent-increase"
  | "divider"
  | "delete"
  | "bold"
  | "italic"
  | "underline"
  | "strike";

export const TableIcon = ({ className }: IconProps) => (
  <svg {...base} className={className} aria-hidden="true">
    <rect x="2.5" y="3" width="11" height="10" rx="1" />
    <path d="M2.5 6.5h11M2.5 9.5h11M6.2 3v10M9.8 3v10" />
  </svg>
);

export const LinkIcon = ({ className }: IconProps) => (
  <svg {...base} className={className} aria-hidden="true">
    <path d="M6.2 9.8 9.8 6.2" />
    <path d="m5.1 11.8-1 1a2.5 2.5 0 0 1-3.5-3.5l2.6-2.6a2.5 2.5 0 0 1 3.5 0" />
    <path d="m10.9 4.2 1-1a2.5 2.5 0 0 1 3.5 3.5l-2.6 2.6a2.5 2.5 0 0 1-3.5 0" />
  </svg>
);

export const DividerIcon = ({ className }: IconProps) => (
  <svg {...base} className={className} aria-hidden="true">
    <path d="M2.5 5h11M2.5 8h11M2.5 11h7" />
  </svg>
);

export const QuoteIcon = ({ className }: IconProps) => (
  <svg {...base} className={className} aria-hidden="true">
    <path d="M3 5.2h3.4v3.4H3zM9.6 5.2H13v3.4H9.6z" />
    <path d="M3 8.6c0 1.7-.3 2.4-1 3.2M9.6 8.6c0 1.7-.3 2.4-1 3.2" />
  </svg>
);

export const CodeIcon = ({ className }: IconProps) => (
  <svg {...base} className={className} aria-hidden="true">
    <path d="m5.5 4-3 4 3 4M10.5 4l3 4-3 4M9 2.8 7 13.2" />
  </svg>
);

export const TrashIcon = ({ className }: IconProps) => (
  <svg {...base} className={className} aria-hidden="true">
    <path d="M3.5 5.5v7a1 1 0 0 0 1 1h7a1 1 0 0 0 1-1v-7" />
    <path d="M2.5 4h11M6 4V2.5h4V4M6.5 7v4M9.5 7v4" />
  </svg>
);

export const ToolbarIcon = ({ name, className }: { name: ToolbarIconName; className?: string }) => {
  switch (name) {
    case "bold": return <span className={`toolbar-icon toolbar-icon--bold${className ? ` ${className}` : ""}`} aria-hidden="true">B</span>;
    case "italic": return <span className={`toolbar-icon toolbar-icon--italic${className ? ` ${className}` : ""}`} aria-hidden="true">I</span>;
    case "underline": return <span className={`toolbar-icon toolbar-icon--underline${className ? ` ${className}` : ""}`} aria-hidden="true">U</span>;
    case "strike": return <span className={`toolbar-icon toolbar-icon--strike${className ? ` ${className}` : ""}`} aria-hidden="true">S</span>;
    case "undo": return <UndoIcon className={className} />;
    case "redo": return <RedoIcon className={className} />;
    case "insert": return <InsertIcon className={className} />;
    case "table": return <TableIcon className={className} />;
    case "link": return <LinkIcon className={className} />;
    case "align-left": return <AlignIcon align="left" className={className} />;
    case "align-center": return <AlignIcon align="center" className={className} />;
    case "align-right": return <AlignIcon align="right" className={className} />;
    case "align-justify": return <AlignIcon align="justify" className={className} />;
    case "bullet-list": return <ListIcon ordered={false} className={className} />;
    case "ordered-list": return <ListIcon ordered className={className} />;
    case "indent-decrease": return <IndentIcon increase={false} className={className} />;
    case "indent-increase": return <IndentIcon increase className={className} />;
    case "divider": return <DividerIcon className={className} />;
    case "delete": return <TrashIcon className={className} />;
  }
};
