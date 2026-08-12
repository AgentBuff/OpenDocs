import { tokenizeCode, type CodeToken } from "./codeConfig.js";

interface HighlightRequest {
  id: number;
  source: string;
  language: Parameters<typeof tokenizeCode>[1];
}

interface HighlightResponse {
  id: number;
  tokens: CodeToken[];
}

type HighlightWorkerScope = typeof globalThis & {
  onmessage: ((event: MessageEvent<HighlightRequest>) => void) | null;
  postMessage: (message: HighlightResponse) => void;
};

const scope = globalThis as HighlightWorkerScope;

scope.onmessage = (event) => {
  const { id, source, language } = event.data;
  scope.postMessage({ id, tokens: tokenizeCode(source, language) });
};

export {};
