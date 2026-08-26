import type { CSSProperties, ReactNode } from "react";

export type IconName =
  | "arrow-down"
  | "arrow-left"
  | "arrow-right"
  | "arrow-up"
  | "align-left"
  | "align-bottom"
  | "align-center"
  | "align-middle"
  | "align-right"
  | "align-top"
  | "align-justify"
  | "bold"
  | "block-handle"
  | "bring-forward"
  | "bring-front"
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
  | "distribute-horizontal"
  | "distribute-vertical"
  | "download"
  | "font-decrease"
  | "font-increase"
  | "flip"
  | "font-colors"
  | "highlight"
  | "history"
  | "image"
  | "delete"
  | "ellipsis"
  | "eraser"
  | "insert"
  | "italic"
  | "line-height"
  | "link"
  | "lock"
  | "merge-cells"
  | "split-cells"
  | "table-borders"
  | "insert-row-column"
  | "ordered-list"
  | "plus"
  | "redo"
  | "restore"
  | "search"
  | "send-back"
  | "send-backward"
  | "shape"
  | "settings"
  | "strikethrough"
  | "table"
  | "todo"
  | "text-extract"
  | "text"
  | "underline"
  | "unlock"
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
