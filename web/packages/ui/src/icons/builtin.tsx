import type { ReactNode } from "react";
import type { IconName, IconProps, IconRenderer } from "./types.js";

// The edit glyphs use the checked-in Arco outline catalog geometry (48×48)
// and are normalized by the registry renderer. Product code only references
// semantic names; SVG path data never lives in toolbar components.

function svg({
  size = 16,
  className,
  title,
  style,
  children,
  viewBox = "0 0 16 16",
  strokeWidth = 1.5,
  strokeLinecap = "round",
  strokeLinejoin = "round",
}: IconProps & {
  children: ReactNode;
  viewBox?: string;
  strokeWidth?: number;
  strokeLinecap?: "butt" | "round" | "square";
  strokeLinejoin?: "miter" | "round" | "bevel";
}) {
  return (
    <svg
      className={className}
      width={size}
      height={size}
      viewBox={viewBox}
      fill="none"
      stroke="currentColor"
      strokeWidth={strokeWidth}
      strokeLinecap={strokeLinecap}
      strokeLinejoin={strokeLinejoin}
      style={style}
      role={title ? "img" : undefined}
      aria-hidden={title ? undefined : "true"}
      aria-label={title}
    >
      {title && <title>{title}</title>}
      {children}
    </svg>
  );
}

export const builtinIcons: Record<IconName, IconRenderer> = {
  "arrow-down": (props) => svg({ ...props, children: <path d="m4 6 4 4 4-4" /> }),
  "arrow-left": (props) => svg({ ...props, children: <path d="m9.5 3.5-4.5 4.5 4.5 4.5M5.5 8h7" /> }),
  "arrow-right": (props) => svg({ ...props, children: <path d="m6.5 3.5 4.5 4.5-4.5 4.5M10.5 8h-7" /> }),
  "arrow-up": (props) => svg({ ...props, children: <path d="m4 10 4-4 4 4" /> }),
  "align-left": (props) => svg({ ...props, children: <><path d="M2.5 3.5h11M2.5 6.5h7M2.5 9.5h11M2.5 12.5h7" /></> }),
  "align-center": (props) => svg({ ...props, children: <><path d="M2.5 3.5h11M4.5 6.5h7M2.5 9.5h11M4.5 12.5h7" /></> }),
  "align-right": (props) => svg({ ...props, children: <><path d="M2.5 3.5h11M6.5 6.5h7M2.5 9.5h11M6.5 12.5h7" /></> }),
  "align-justify": (props) => svg({ ...props, children: <><path d="M2.5 3.5h11M2.5 6.5h11M2.5 9.5h11M2.5 12.5h11" /></> }),
  bold: (props) => svg({ ...props, children: <path d="M4.5 2.5h3.1a2.4 2.4 0 0 1 .4 4.8H4.5m3.5 0a2.6 2.6 0 0 1 0 5.2H4.5v-10" /> }),
  "block-handle": (props) => svg({ ...props, children: <><circle cx="5" cy="4" r="1.05" fill="currentColor" stroke="none" /><circle cx="11" cy="4" r="1.05" fill="currentColor" stroke="none" /><circle cx="5" cy="8" r="1.05" fill="currentColor" stroke="none" /><circle cx="11" cy="8" r="1.05" fill="currentColor" stroke="none" /><circle cx="5" cy="12" r="1.05" fill="currentColor" stroke="none" /><circle cx="11" cy="12" r="1.05" fill="currentColor" stroke="none" /></> }),
  "bg-colors": (props) => svg({
    ...props,
    viewBox: "0 0 48 48",
    strokeWidth: 4,
    strokeLinecap: "butt",
    children: <>
      <path d="M9.44231 25.25L19.7932 36.0149C20.1813 36.4185 20.8252 36.4248 21.2211 36.0289L32 25.25H9.44231Z" fill="currentColor" stroke="none" />
      <path d="M19 5.25L22.75 9M22.75 9L34.7929 21.0429C35.1834 21.4334 35.1834 22.0666 34.7929 22.4571L32 25.25M22.75 9L8.69338 23.0566C8.30826 23.4417 8.30215 24.0642 8.67965 24.4568L9.44231 25.25M9.44231 25.25L19.7932 36.0149C20.1813 36.4185 20.8252 36.4248 21.2211 36.0289L32 25.25M9.44231 25.25H32" />
      <path fillRule="evenodd" clipRule="evenodd" d="M40.0134 29.8123L37.2012 27L34.3889 29.8123C33.6555 30.5374 33.2012 31.5439 33.2012 32.6567C33.2012 34.8658 34.9921 36.6567 37.2012 36.6567C39.4103 36.6567 41.2012 34.8658 41.2012 32.6567C41.2012 31.5439 40.7468 30.5374 40.0134 29.8123Z" fill="currentColor" stroke="none" />
    </>,
  }),
  "bullet-list": (props) => svg({ ...props, children: <><path d="M6.5 4h7M6.5 8h7M6.5 12h7" /><circle cx="3" cy="4" r=".8" fill="currentColor" stroke="none" /><circle cx="3" cy="8" r=".8" fill="currentColor" stroke="none" /><circle cx="3" cy="12" r=".8" fill="currentColor" stroke="none" /></> }),
  brush: (props) => svg({
    ...props,
    viewBox: "0 0 48 48",
    strokeWidth: 4,
    strokeLinecap: "butt",
    children: <>
      <path d="M33 13H40.0002C40.5525 13 41 13.4477 41 14V26.1407C41 26.6349 40.639 27.0549 40.1504 27.1293L18.8496 30.3707C18.361 30.4451 18 30.8651 18 31.3593V43" />
      <path d="M33 7.99947C33 10.0324 33 14.7332 33 18.002C33 18.5543 32.5523 19 32 19H8C7.44772 19 7 18.5523 7 18V8C7 7.44772 7.4444 7 7.99668 7C15.598 7 28.3768 7 32.0097 7C32.562 7 33 7.44718 33 7.99947Z" />
    </>,
  }),
  check: (props) => svg({ ...props, children: <path d="m3 8.5 3 3 7-7" /> }),
  close: (props) => svg({ ...props, children: <path d="m4 4 8 8M12 4l-8 8" /> }),
  code: (props) => svg({ ...props, children: <path d="m5.5 4-3 4 3 4M10.5 4l3 4-3 4M9 2.8 7 13.2" /> }),
  crop: (props) => svg({ ...props, children: <><path d="M5 2.5v8a1 1 0 0 0 1 1h7.5M11 13.5V5.5a1 1 0 0 0-1-1H2.5" /><path d="M11.5 11.5h2v2" /></> }),
  caption: (props) => svg({ ...props, children: <><rect x="2.5" y="3" width="11" height="8" rx="1" /><path d="M4.5 13h7M5.2 6h5.6M5.2 8h3.6" /></> }),
  compress: (props) => svg({ ...props, children: <><path d="M2.8 6.4V3.2H6M10 3.2h3.2v3.2M13.2 9.6v3.2H10M6 12.8H2.8V9.6" /><path d="m6 5 4 3-4 3" /></> }),
  copy: (props) => svg({ ...props, children: <><rect x="5.5" y="5.5" width="8" height="8" rx="1" /><path d="M10.5 5.5v-2a1 1 0 0 0-1-1h-6a1 1 0 0 0-1 1v6a1 1 0 0 0 1 1h2" /></> }),
  divider: (props) => svg({ ...props, children: <path d="M2.5 5h11M2.5 8h11M2.5 11h7" /> }),
  download: (props) => svg({ ...props, children: <><path d="M8 2.5v7" /><path d="m5.2 7.2 2.8 2.8 2.8-2.8" /><path d="M3 11.5v1a1 1 0 0 0 1 1h8a1 1 0 0 0 1-1v-1" /></> }),
  delete: (props) => svg({ ...props, children: <><path d="M3.5 5.5v7a1 1 0 0 0 1 1h7a1 1 0 0 0 1-1v-7M2.5 4h11M6 4V2.5h4V4M6.5 7v4M9.5 7v4" /></> }),
  "font-decrease": (props) => svg({ ...props, children: <><path d="M4 12 7 4l3 8M5 9.5h4" /><path d="M11.5 10h3" /></> }),
  "font-increase": (props) => svg({ ...props, children: <><path d="M4 12 7 4l3 8M5 9.5h4" /><path d="M12.5 8v4M10.5 10h4" /></> }),
  flip: (props) => svg({ ...props, children: <><path d="M2.5 3.5h11v9h-11z" /><path d="M8 3.5v9M5.8 6.3 3.8 8l2 1.7M10.2 6.3 12.2 8l-2 1.7" /></> }),
  "font-colors": (props) => svg({
    ...props,
    viewBox: "0 0 48 48",
    strokeWidth: 4,
    strokeLinecap: "butt",
    children: <path d="M16.467 22L11.5 34M31.5315 22L24.9993 7H22.9993L16.467 22H31.5315ZM31.5315 22H16.467H31.5315ZM31.5315 22L36.5 34L31.5315 22Z" />,
  }),
  eraser: (props) => svg({
    ...props,
    viewBox: "0 0 48 48",
    strokeWidth: 4,
    strokeLinecap: "butt",
    children: <><path d="M25.5 40.5034L14.914 40.5001C14.6489 40.5 14.3947 40.3947 14.2072 40.2072L5.20711 31.2071C4.81658 30.8166 4.81658 30.1834 5.20711 29.7929L13.5 21.5M25.5 40.5034L44 40.5M25.5 40.5034L29 37M13.5 21.5L26.7929 8.20711C27.1834 7.81658 27.8166 7.81658 28.2071 8.20711L42.2929 22.2929C42.6834 22.6834 42.6834 23.3166 42.2929 23.7071L29 37M13.5 21.5L29 37" /></>,
  }),
  highlight: (props) => svg({
    ...props,
    viewBox: "0 0 48 48",
    strokeWidth: 4,
    strokeLinecap: "butt",
    children: <path d="M19 18V9.28078C19 8.82191 19.3123 8.42193 19.7575 8.31063L27.7575 6.31063C28.3886 6.15285 29 6.63021 29 7.28078V18M19 18H15C14.4477 18 14 18.4477 14 19V27H10C9.44772 27 9 27.4477 9 28V43M19 18H29M29 18H33C33.5523 18 34 18.4477 34 19V27H38C38.5523 27 39 27.4477 39 28V43" />,
  }),
  history: (props) => svg({ ...props, children: <><circle cx="8" cy="8" r="5.5" /><path d="M8 4.5v3.8l2.4 1.4M2.5 5.5V3.2M2.5 3.2h2.3" /></> }),
  ellipsis: (props) => svg({ ...props, children: <><circle cx="3.5" cy="8" r=".8" fill="currentColor" stroke="none" /><circle cx="8" cy="8" r=".8" fill="currentColor" stroke="none" /><circle cx="12.5" cy="8" r=".8" fill="currentColor" stroke="none" /></> }),
  insert: (props) => svg({ ...props, children: <><circle cx="8" cy="8" r="5.5" /><path d="M8 5v6M5 8h6" /></> }),
  "insert-row-column": (props) => svg({ ...props, children: <><rect x="2.5" y="3" width="8" height="10" rx=".8" /><path d="M2.5 6.3h8M6.5 3v10M13 7.8v5M10.5 10.3h5" /></> }),
  italic: (props) => svg({ ...props, children: <path d="M7 2.5h5M4 13.5h5M9.5 2.5l-3 11" /> }),
  "line-height": (props) => svg({ ...props, children: <><path d="M6.5 4h7M6.5 8h7M6.5 12h7M3 5.5v5" /><path d="M1.8 6.8 3 5.5l1.2 1.3M1.8 9.2 3 10.5l1.2-1.3" /></> }),
  link: (props) => svg({ ...props, children: <><path d="M6.2 9.8 9.8 6.2" /><path d="m5.1 11.8-1 1a2.5 2.5 0 0 1-3.5-3.5l2.6-2.6a2.5 2.5 0 0 1 3.5 0" /><path d="m10.9 4.2 1-1a2.5 2.5 0 0 1 3.5 3.5l-2.6 2.6a2.5 2.5 0 0 1-3.5 0" /></> }),
  "merge-cells": (props) => svg({ ...props, children: <><rect x="2.5" y="4" width="11" height="8" rx=".75" /><path d="M8 4v8M5.8 8h4.4M5.8 8 7 6.8M5.8 8 7 9.2M10.2 8 9 6.8M10.2 8 9 9.2" /></> }),
  "split-cells": (props) => svg({ ...props, children: <><rect x="2.5" y="4" width="11" height="8" rx=".75" /><path d="M8 4v8M7 8H4.6M4.6 8l1.2-1.2M4.6 8l1.2 1.2M9 8h2.4M11.4 8l-1.2-1.2M11.4 8l-1.2 1.2" /></> }),
  "ordered-list": (props) => svg({ ...props, children: <><path d="M6.5 4h7M6.5 8h7M6.5 12h7" /><path d="M2.5 3h1v2M2.3 7h1.8l-1.8 2h1.8M2.3 11h1.5v2H2.3" strokeWidth="1.1" /></> }),
  plus: (props) => svg({ ...props, children: <path d="M8 3v10M3 8h10" /> }),
  quote: (props) => svg({ ...props, children: <><path d="M3 5.2h3.4v3.4H3zM9.6 5.2H13v3.4H9.6z" /><path d="M3 8.6c0 1.7-.3 2.4-1 3.2M9.6 8.6c0 1.7-.3 2.4-1 3.2" /></> }),
  redo: (props) => svg({ ...props, children: <><path d="M13 8H6a3 3 0 0 0 0 6h3" /><path d="m10.5 5.5 2.5 2.5-2.5 2.5" /></> }),
  restore: (props) => svg({ ...props, children: <><path d="M3 7.5A5.3 5.3 0 1 1 5 12" /><path d="M3 3.5v4h4" /></> }),
  search: (props) => svg({ ...props, children: <><circle cx="7" cy="7" r="4.5" /><path d="m10.5 10.5 3 3" /></> }),
  settings: (props) => svg({ ...props, children: <><circle cx="8" cy="8" r="2.2" /><path d="m8 2.5.7 1.4 1.5.5 1.4-.6 1.1 1.1-.6 1.4.5 1.5 1.4.7v1.6l-1.4.7-.5 1.5.6 1.4-1.1 1.1-1.4-.6-1.5.5-.7 1.4H6.4l-.7-1.4-1.5-.5-1.4.6-1.1-1.1.6-1.4-.5-1.5-1.4-.7V8.4l1.4-.7.5-1.5-.6-1.4 1.1-1.1 1.4.6 1.5-.5.7-1.4H8Z" /></> }),
  strikethrough: (props) => svg({ ...props, children: <><path d="M3 8h10M5 5.5c.6-2.6 5.4-2.7 6.2-.2M11 10.5c-.6 2.6-5.4 2.7-6.2.2" /></> }),
  table: (props) => svg({ ...props, children: <><rect x="2.5" y="3" width="11" height="10" rx="1" /><path d="M2.5 6.5h11M2.5 9.5h11M6.2 3v10M9.8 3v10" /></> }),
  "table-borders": (props) => svg({ ...props, children: <><rect x="2.5" y="2.5" width="11" height="11" rx=".75" /><path d="M2.5 6.2h11M2.5 9.8h11M6.2 2.5v11M9.8 2.5v11" /><path d="M2.5 2.5h11v11h-11z" strokeWidth="2" /></> }),
  todo: (props) => svg({ ...props, children: <><rect x="2.5" y="2.5" width="11" height="11" rx="1.25" /><path d="m5 8 2 2 4-4" /></> }),
  "text-extract": (props) => svg({ ...props, children: <><rect x="2.5" y="2.5" width="11" height="11" rx="1" /><path d="M5 5.5h6M5 8h4.5M5 10.5h3" /></> }),
  underline: (props) => svg({ ...props, children: <><path d="M4.5 2.5v5a3.5 3.5 0 0 0 7 0v-5M3 13.5h10" /></> }),
  undo: (props) => svg({ ...props, children: <><path d="M3 8h7a3 3 0 0 1 0 6H7" /><path d="m5.5 5.5-2.5 2.5 2.5 2.5" /></> }),
  "vertical-align": (props) => svg({ ...props, children: <><path d="M6 3.5h7M6 8h7M6 12.5h7" /><path d="M2.5 5.5V10.5M1 7l1.5-1.5L4 7M1 9l1.5 1.5L4 9" /></> }),
};
