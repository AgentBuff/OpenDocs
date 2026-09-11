import type {
  ColorRef,
  Paint,
  PresentationV5Deck,
  PresentationV5RichText,
  PresentationV5SlideBackground,
  PresentationV5Transform,
} from "@open-office/schema";

import { colorCss } from "./presentationGeometry.js";
import { MIN_PRESENTATION_NODE_SIZE } from "./interactions.js";

export function asSlideBackground(value: unknown): PresentationV5SlideBackground {
  if (value && typeof value === "object" && (value as { type?: unknown }).type === "solid") {
    return value as PresentationV5SlideBackground;
  }
  return { type: "none" };
}

export function pageSafeArea(value: unknown): PresentationV5Deck["pageSpec"]["safeArea"] {
  if (!value || typeof value !== "object") return null;
  const candidate = value as Partial<Record<"top" | "right" | "bottom" | "left", unknown>>;
  const edges = [candidate.top, candidate.right, candidate.bottom, candidate.left];
  if (!edges.every((edge) => typeof edge === "number" && Number.isFinite(edge) && edge >= 0)) return null;
  return { top: candidate.top as number, right: candidate.right as number, bottom: candidate.bottom as number, left: candidate.left as number };
}

export function themeIdForName(name: string): string {
  const normalized = name.trim().toLowerCase().replace(/[^a-z0-9]+/g, "-").replace(/^-|-$/g, "");
  return normalized || "custom-theme";
}

export function colorInputValue(paint: Paint): string {
  if (paint.type !== "solid") return "#ffffff";
  const css = colorCss(paint.value);
  if (!css) return "#ffffff";
  if (/^#[\da-f]{6}$/i.test(css)) return css;
  const rgba = /^rgba\((\d+), (\d+), (\d+),/.exec(css);
  if (!rgba) return "#ffffff";
  return `#${[rgba[1], rgba[2], rgba[3]].map((part) => Number(part).toString(16).padStart(2, "0")).join("")}`;
}

export function colorRefInputValue(color: ColorRef | null): string {
  return color ? colorInputValue({ type: "solid", value: color }) : "#000000";
}

export function solidColor(hex: string): ColorRef {
  const normalized = /^#[\da-f]{6}$/i.test(hex) ? hex.slice(1) : "000000";
  return {
    type: "rgba",
    value: {
      r: Number.parseInt(normalized.slice(0, 2), 16),
      g: Number.parseInt(normalized.slice(2, 4), 16),
      b: Number.parseInt(normalized.slice(4, 6), 16),
      a: 255,
    },
  };
}

export function positiveNumberOrNull(value: string): number | null {
  if (!value.trim()) return null;
  const parsed = Number(value);
  return Number.isFinite(parsed) && parsed > 0 && parsed <= 512 ? parsed : null;
}

export function textStyleForInspector(body: PresentationV5RichText): PresentationV5RichText["runs"][number]["style"] {
  return body.runs[0]?.style ?? {
    bold: false,
    italic: false,
    underline: false,
    strikethrough: false,
    fontFamily: null,
    fontSize: null,
    color: null,
  };
}

export function solidPaint(hex: string): Paint {
  return {
    type: "solid",
    value: solidColor(hex),
  };
}

export function isUsableTransform(transform: PresentationV5Transform): boolean {
  return Number.isFinite(transform.x)
    && Number.isFinite(transform.y)
    && Number.isFinite(transform.rotation)
    && Number.isFinite(transform.width)
    && Number.isFinite(transform.height)
    && transform.width >= MIN_PRESENTATION_NODE_SIZE
    && transform.height >= MIN_PRESENTATION_NODE_SIZE;
}
