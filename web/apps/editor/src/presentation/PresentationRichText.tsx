import { useLayoutEffect, useRef, useState, type CSSProperties } from "react";
import { presentationParagraphsForText, type PresentationV5RichText } from "@open-office/schema";
import { preserveTextRuns } from "../typography/preserve-text-runs.js";

type TextStyle = PresentationV5RichText["runs"][number]["style"];
const DEFAULT_TEXT_STYLE: TextStyle = { fontFamily: null, fontSize: null, color: null, bold: false, italic: false, underline: false, strikethrough: false };

export function withPresentationFont(body: PresentationV5RichText, fontFamily: string): PresentationV5RichText {
  if (!body.text) return body;
  return { ...body, runs: body.runs.length
    ? body.runs.map(run => ({ ...run, style: { ...run.style, fontFamily } }))
    : [{ start: 0, end: [...body.text].length, style: { fontFamily, fontSize: null, color: null, bold: false, italic: false, underline: false, strikethrough: false } }],
  };
}

export function preservePresentationText(body: PresentationV5RichText, text: string): PresentationV5RichText {
  const next = preserveTextRuns(body, text);
  return { ...next, paragraphs: presentationParagraphsForText(text, body.paragraphs) };
}

export function patchPresentationTextRange(
  body: PresentationV5RichText,
  start: number,
  end: number,
  patch: Partial<TextStyle>,
): PresentationV5RichText {
  const length = [...body.text].length;
  const rangeStart = Math.max(0, Math.min(length, start));
  const rangeEnd = Math.max(rangeStart, Math.min(length, end));
  if (rangeStart === rangeEnd) return body;
  const source = body.runs.length ? body.runs : length ? [{ start: 0, end: length, style: DEFAULT_TEXT_STYLE }] : [];
  const runs: PresentationV5RichText["runs"] = [];
  const append = (run: PresentationV5RichText["runs"][number]) => {
    if (run.end <= run.start) return;
    const previous = runs.at(-1);
    if (previous && previous.end === run.start && JSON.stringify(previous.style) === JSON.stringify(run.style)) previous.end = run.end;
    else runs.push(run);
  };
  for (const run of source) {
    append({ ...run, end: Math.min(run.end, rangeStart) });
    const overlapStart = Math.max(run.start, rangeStart);
    const overlapEnd = Math.min(run.end, rangeEnd);
    if (overlapStart < overlapEnd) append({ start: overlapStart, end: overlapEnd, style: { ...run.style, ...patch } });
    append({ ...run, start: Math.max(run.start, rangeEnd) });
  }
  return { ...body, runs };
}

export function patchPresentationParagraphRange(
  body: PresentationV5RichText,
  start: number,
  end: number,
  patch: Partial<Pick<PresentationV5RichText["paragraphs"][number], "alignment" | "list" | "indentLevel">>,
): PresentationV5RichText {
  const rangeStart = Math.max(0, Math.min([...body.text].length, start));
  const rangeEnd = Math.max(rangeStart, Math.min([...body.text].length, end));
  const paragraphs = body.paragraphs.map((paragraph) => {
    const selected = rangeStart === rangeEnd
      ? paragraph.start <= rangeStart && rangeStart <= paragraph.end
      : paragraph.start < rangeEnd && paragraph.end > rangeStart;
    return selected ? { ...paragraph, ...patch } : paragraph;
  });
  return { ...body, paragraphs };
}

export function presentationRangeStyle(
  body: PresentationV5RichText,
  start: number,
  end: number,
): TextStyle {
  const length = [...body.text].length;
  const rangeStart = Math.max(0, Math.min(length, start));
  const rangeEnd = Math.max(rangeStart, Math.min(length, end));
  const source = body.runs.length ? body.runs : length ? [{ start: 0, end: length, style: DEFAULT_TEXT_STYLE }] : [];
  const relevant = source.filter((run) => rangeStart === rangeEnd
    ? run.start <= rangeStart && rangeStart < run.end
    : run.start < rangeEnd && run.end > rangeStart);
  if (!relevant.length) return DEFAULT_TEXT_STYLE;
  return relevant.reduce((shared, run) => ({
    ...shared,
    bold: shared.bold && run.style.bold,
    italic: shared.italic && run.style.italic,
    underline: shared.underline && run.style.underline,
    strikethrough: shared.strikethrough && run.style.strikethrough,
  }), relevant[0]!.style);
}

/** Presentation sizes are points; geometry is EMU (12,700 EMU per point). */
export function presentationTextStyle(style: TextStyle | undefined, pointScale: number | string = 4 / 3): CSSProperties {
  if (!style) return {};
  const color = style.color;
  const themes = { background: "#fff", text: "#192033", accent1: "#2458d3", accent2: "#17a88b", accent3: "#ef9f28", accent4: "#8b5cf6", accent5: "#ef5e8d", accent6: "#40a9ff", hyperlink: "#2458d3", followedHyperlink: "#7c4ec2" };
  return {
    fontFamily: style.fontFamily ?? undefined,
    fontSize: style.fontSize == null ? undefined : typeof pointScale === "number" ? style.fontSize * pointScale : `calc(${style.fontSize} * ${pointScale})`,
    fontWeight: style.bold ? 700 : undefined,
    fontStyle: style.italic ? "italic" : undefined,
    color: !color ? undefined : color.type === "rgba" ? `rgba(${color.value.r}, ${color.value.g}, ${color.value.b}, ${color.value.a / 255})` : themes[color.value],
    textDecoration: [style.underline ? "underline" : "", style.strikethrough ? "line-through" : ""].filter(Boolean).join(" ") || undefined,
  };
}

export function PresentationRichText({ body, pointScale = 4 / 3 }: { body: PresentationV5RichText; pointScale?: number | string }) {
  const characters = [...body.text];
  if (!body.paragraphs.length) return <>{body.text}</>;
  const orderedCounters = new Map<number, number>();
  return <>{body.paragraphs.map((paragraph) => {
    const end = characters[paragraph.end - 1] === "\n" ? paragraph.end - 1 : paragraph.end;
    const runs = body.runs.flatMap((run) => {
      const start = Math.max(run.start, paragraph.start);
      const runEnd = Math.min(run.end, end);
      return start < runEnd ? [{ ...run, start, end: runEnd }] : [];
    });
    const content = runs.length
      ? runs.map((run) => <span key={run.start} style={presentationTextStyle(run.style, pointScale)}>{characters.slice(run.start, run.end).join("")}</span>)
      : characters.slice(paragraph.start, end).join("");
    let marker: string | null = null;
    if (paragraph.list?.type === "bullet") {
      marker = "•";
      orderedCounters.delete(paragraph.indentLevel);
    } else if (paragraph.list?.type === "ordered") {
      const value = orderedCounters.get(paragraph.indentLevel) ?? paragraph.list.startAt;
      marker = `${value}.`;
      orderedCounters.set(paragraph.indentLevel, value + 1);
    } else {
      orderedCounters.clear();
    }
    return <div key={paragraph.start} className="presentation-studio__paragraph" style={{ textAlign: paragraph.alignment, paddingLeft: `${paragraph.indentLevel * 1.5}em` }}>
      {marker && <span className="presentation-studio__list-marker">{marker}</span>}{content || <br />}
    </div>;
  })}</>;
}

export function PresentationTextFrame({ body, pointScale, autoFit, verticalAlign, padding, className, style }: {
  body: PresentationV5RichText;
  pointScale?: number | string;
  autoFit: "none" | "shrinkText" | "resizeShape";
  verticalAlign: "top" | "middle" | "bottom";
  padding: string;
  className?: string;
  style?: CSSProperties;
}) {
  const frameRef = useRef<HTMLDivElement>(null);
  const contentRef = useRef<HTMLDivElement>(null);
  const [textScale, setTextScale] = useState(1);
  useLayoutEffect(() => {
    const frame = frameRef.current;
    const content = contentRef.current;
    if (!frame || !content || autoFit !== "shrinkText") {
      setTextScale(1);
      return;
    }
    const fit = () => {
      const availableWidth = Math.max(1, frame.clientWidth);
      const availableHeight = Math.max(1, frame.clientHeight);
      const widthScale = availableWidth / Math.max(availableWidth, content.scrollWidth);
      const heightScale = availableHeight / Math.max(availableHeight, content.scrollHeight);
      setTextScale(Math.max(0.1, Math.min(1, widthScale, heightScale)));
    };
    fit();
    const observer = new ResizeObserver(fit);
    observer.observe(frame);
    observer.observe(content);
    return () => observer.disconnect();
  }, [autoFit, body, padding, pointScale]);
  const justifyContent = verticalAlign === "top" ? "flex-start" : verticalAlign === "bottom" ? "flex-end" : "center";
  return <div ref={frameRef} className={`${className ?? ""} presentation-studio__text-content presentation-studio__text-content--${autoFit}`} style={{ ...style, justifyContent, padding }}>
    <div ref={contentRef} className="presentation-studio__text-fit-content" style={{ transform: `scale(${textScale})` }}><PresentationRichText body={body} pointScale={pointScale} /></div>
  </div>;
}
