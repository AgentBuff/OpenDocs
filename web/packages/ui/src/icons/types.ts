import type { CSSProperties, ReactNode } from "react";

export type IconName =
  | "arrow-down"
  | "arrow-left"
  | "arrow-right"
  | "arrow-up"
  | "align-left"
  | "align-center"
  | "align-right"
  | "align-justify"
  | "bold"
  | "block-handle"
  | "bullet-list"
  | "bg-colors"
  | "brush"
  | "check"
  | "close"
  | "code"
  | "crop"
  | "caption"
  | "compress"
  | "copy"
  | "divider"
  | "download"
  | "font-decrease"
  | "font-increase"
  | "flip"
  | "font-colors"
  | "highlight"
  | "history"
  | "delete"
  | "ellipsis"
  | "eraser"
  | "insert"
  | "italic"
  | "line-height"
  | "link"
  | "merge-cells"
  | "split-cells"
  | "table-borders"
  | "insert-row-column"
  | "ordered-list"
  | "plus"
  | "redo"
  | "restore"
  | "search"
  | "settings"
  | "strikethrough"
  | "table"
  | "todo"
  | "text-extract"
  | "underline"
  | "undo"
  | "vertical-align"
  | "quote";

export interface IconProps {
  size?: number | string;
  className?: string;
  title?: string;
  style?: CSSProperties;
}

export type IconRenderer = (props: IconProps) => ReactNode;

export interface IconRegistry {
  version: string;
  resolve(name: IconName): IconRenderer | undefined;
}
