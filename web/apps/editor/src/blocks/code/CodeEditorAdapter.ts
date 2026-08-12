import type { CodeBlockConfig, RichText } from "@open-office/schema/artifact";

/**
 * Editor surface contract for code blocks. The adapter owns transient cursor,
 * selection and highlight state; only committed source/config go through the
 * BlockSession command API. This keeps a future worker/editor swap out of the
 * document schema and React Block tree.
 */
export interface CodeEditorAdapter {
  readonly source: string;
  readonly config: CodeBlockConfig;
  setSource(source: string): void;
  setConfig(config: CodeBlockConfig): void;
  focus(): void;
  destroy(): void;
}

export interface CodeEditorCommit {
  content: RichText;
  config: CodeBlockConfig;
}

export function plainCodeContent(source: string): RichText {
  return { text: source, runs: [] };
}
