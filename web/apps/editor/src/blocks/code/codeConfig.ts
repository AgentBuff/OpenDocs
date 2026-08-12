import type { CodeBlockConfig, CodeLanguage, CodeTheme } from "@open-office/schema/artifact";

export const CODE_LANGUAGES: ReadonlyArray<{ id: CodeLanguage; label: string }> = [
  { id: "plainText", label: "Plain Text" },
  { id: "javascript", label: "JavaScript" },
  { id: "typescript", label: "TypeScript" },
  { id: "rust", label: "Rust" },
  { id: "python", label: "Python" },
  { id: "java", label: "Java" },
  { id: "json", label: "JSON" },
  { id: "html", label: "HTML" },
  { id: "css", label: "CSS" },
  { id: "sql", label: "SQL" },
  { id: "bash", label: "Bash" },
  { id: "markdown", label: "Markdown" },
];

export const CODE_THEMES: ReadonlyArray<{ id: CodeTheme; label: string }> = [
  { id: "light", label: "Open Office Light" },
  { id: "dark", label: "Open Office Dark" },
];

/** Persisted viewport bounds for a code block. Content beyond the viewport scrolls. */
export const CODE_BLOCK_MIN_HEIGHT = 160;
export const CODE_BLOCK_MAX_HEIGHT = 640;

export function clampCodeBlockHeight(value: number): number {
  return Math.min(CODE_BLOCK_MAX_HEIGHT, Math.max(CODE_BLOCK_MIN_HEIGHT, Math.round(value)));
}

export function codeConfig(value: CodeBlockConfig): CodeBlockConfig {
  return { ...value };
}

export function indentText(config: CodeBlockConfig): string {
  return config.indentMode === "tabs" ? "\t" : " ".repeat(config.indentWidth);
}

/**
 * Counts logical source lines without allocating the complete `split()` array.
 * The editor uses this value for the gutter, so keeping the operation allocation
 * free matters when a large code block is edited on every keystroke.
 */
export function countCodeLines(source: string): number {
  let count = 1;
  for (let index = 0; index < source.length; index += 1) {
    if (source.charCodeAt(index) === 10) count += 1;
  }
  return count;
}

/**
 * Produces one text node worth of line numbers. Rendering a `<span>` per line
 * makes a 10k-line block needlessly expensive; the gutter is presentation-only
 * and does not need individual DOM nodes.
 */
export function codeLineNumbers(source: string): string {
  const count = countCodeLines(source);
  let numbers = "1";
  for (let line = 2; line <= count; line += 1) numbers += `\n${line}`;
  return numbers;
}

/** A response is safe to commit only while its request is still current. */
export function isCurrentHighlightRequest(responseId: number, currentRequestId: number, disposed: boolean): boolean {
  return !disposed && responseId === currentRequestId;
}

export interface CodeToken {
  text: string;
  className?: string;
}

/**
 * A small, allocation-bounded baseline tokenizer for the first code-block release.
 * It is intentionally not the persistence model: a future CodeEditorAdapter can replace
 * this tokenizer with a language worker (CodeMirror/Lezer or another implementation)
 * without changing the Block payload or the editor surface.
 */
export function tokenizeCode(source: string, language: CodeLanguage): CodeToken[] {
  if (language === "plainText") return [{ text: source }];
  const tokens: CodeToken[] = [];
  const pattern = /("(?:\\.|[^"\\])*"|'(?:\\.|[^'\\])*'|`(?:\\.|[^`\\])*`|\/\/[^\n]*|#[^\n]*|\b\d+(?:\.\d+)?\b|\b(?:const|let|var|function|return|if|else|for|while|class|new|import|from|export|async|await|fn|pub|struct|impl|use|mod|def|in|True|False|None|null|undefined|SELECT|FROM|WHERE|INSERT|UPDATE|DELETE)\b)/g;
  let offset = 0;
  for (const match of source.matchAll(pattern)) {
    const start = match.index ?? offset;
    if (start > offset) tokens.push({ text: source.slice(offset, start) });
    const text = match[0];
    const className = text.startsWith("//") || text.startsWith("#")
      ? "code-token--comment"
      : /^['"`]/.test(text)
        ? "code-token--string"
        : /^\d/.test(text)
          ? "code-token--number"
          : "code-token--keyword";
    tokens.push({ text, className });
    offset = start + text.length;
  }
  if (offset < source.length) tokens.push({ text: source.slice(offset) });
  return tokens.length > 0 ? tokens : [{ text: source }];
}
