/** 后端 API 的客户端封装。 */

import { parseCommitResult, parseSnapshot } from "@open-office/schema/artifact";
import { DOCUMENT_HISTORY_TYPE_ID, MINDMAP_HISTORY_TYPE_ID } from "@open-office/schema/artifact";
import { ArtifactApiClient, parseArtifactMeta } from "@open-office/schema/api";
import type { EventRequest, ProjectionRequest } from "@open-office/schema/api";
import type { ArtifactMeta as SharedArtifactMeta } from "@open-office/schema/api";
import type {
  ArtifactCommandEnvelope,
  CommitResult,
  DocumentHistoryAction,
} from "@open-office/schema/artifact";

export type ArtifactKind = SharedArtifactMeta["kind"];

export type DocumentMeta = SharedArtifactMeta;

export type ArtifactMeta = DocumentMeta;

const typedArtifactApi = new ArtifactApiClient();

/** 一批可重放的编辑变更，按服务端 revision 严格排序。 */
export interface DocumentCommitResult extends CommitResult {
  document: DocumentMeta;
  canUndo: boolean;
  canRedo: boolean;
}

export interface DocumentHistoryState {
  canUndo: boolean;
  canRedo: boolean;
}

export interface DocumentSnapshotMeta {
  version: number;
  createdAt: string;
  current: boolean;
}

interface ErrorBody {
  error?: string;
  code?: string;
  requestId?: string;
  retryable?: boolean;
  details?: Record<string, unknown>;
}

/** HTTP boundary error with a stable status for retry/conflict policy. */
export class ApiRequestError extends Error {
  readonly status: number;
  readonly code: string | null;
  readonly requestId: string | null;
  readonly retryable: boolean;
  readonly details: Record<string, unknown> | null;

  constructor(
    status: number,
    message: string,
    code: string | null = null,
    requestId: string | null = null,
    retryable = false,
    details: Record<string, unknown> | null = null,
  ) {
    super(message);
    this.name = "ApiRequestError";
    this.status = status;
    this.code = code;
    this.requestId = requestId;
    this.retryable = retryable;
    this.details = details;
  }
}

/** 带上后端返回的可读错误信息，而不是只抛一个状态码。 */
async function request<T>(input: string, init?: RequestInit): Promise<T> {
  const response = await fetch(input, init);
  if (!response.ok) {
    let message = `请求失败（HTTP ${response.status}）`;
    let code: string | null = null;
    let requestId: string | null = response.headers.get("x-request-id");
    let retryable = false;
    let details: Record<string, unknown> | null = null;
    try {
      const body = (await response.json()) as ErrorBody;
      if (body.error) message = body.error;
      if (body.code) code = body.code;
      if (body.requestId) requestId = body.requestId;
      if (typeof body.retryable === "boolean") retryable = body.retryable;
      if (body.details) details = body.details;
    } catch {
      // 响应不是 JSON 时保留默认文案。
    }
    throw new ApiRequestError(response.status, message, code, requestId, retryable, details);
  }
  if (response.status === 204) return undefined as T;
  return (await response.json()) as T;
}

export const api = {
  /** Read-only projections use the framework-free schema client boundary. */
  capabilities: () => typedArtifactApi.capabilities(),
  outline: (id: string, options?: ProjectionRequest) => typedArtifactApi.outline(id, options),
  listBlocks: (id: string, options?: ProjectionRequest) => typedArtifactApi.blocks(id, options),
  block: (id: string, blockId: string, options?: ProjectionRequest) => typedArtifactApi.block(id, blockId, options),
  presentation: (id: string) => typedArtifactApi.presentation(id),
  mindmap: (id: string, theme?: "light" | "dark" | "highContrast") => typedArtifactApi.mindmap(id, theme),
  presence: (id: string) => typedArtifactApi.presentationPresence(id),
  updatePresence: (id: string, sessionId: string, update: Parameters<typeof typedArtifactApi.updatePresentationPresence>[2]) =>
    typedArtifactApi.updatePresentationPresence(id, sessionId, update),
  documentPresence: (id: string) => typedArtifactApi.documentPresence(id),
  updateDocumentPresence: (id: string, sessionId: string, update: Parameters<typeof typedArtifactApi.updateDocumentPresence>[2]) =>
    typedArtifactApi.updateDocumentPresence(id, sessionId, update),
  documentReviews: (id: string) => typedArtifactApi.documentReviews(id),
  createDocumentReview: (id: string, review: Parameters<typeof typedArtifactApi.createDocumentReview>[1]) =>
    typedArtifactApi.createDocumentReview(id, review),
  createDocumentSuggestion: (id: string, suggestion: Parameters<typeof typedArtifactApi.createDocumentSuggestion>[1]) =>
    typedArtifactApi.createDocumentSuggestion(id, suggestion),
  replyDocumentReview: (id: string, threadId: string, message: Parameters<typeof typedArtifactApi.replyDocumentReview>[2]) =>
    typedArtifactApi.replyDocumentReview(id, threadId, message),
  updateDocumentReview: (id: string, threadId: string, state: Parameters<typeof typedArtifactApi.updateDocumentReview>[2]) =>
    typedArtifactApi.updateDocumentReview(id, threadId, state),
  presentationOutline: (id: string, options?: Parameters<typeof typedArtifactApi.presentationOutline>[1]) => typedArtifactApi.presentationOutline(id, options),
  presentationSlide: (id: string, slideId: string, options?: Parameters<typeof typedArtifactApi.presentationSlide>[2]) => typedArtifactApi.presentationSlide(id, slideId, options),
  events: (id: string, options?: EventRequest) => typedArtifactApi.events(id, options),
  uploadAsset: (id: string, file: Blob, fileName?: string) => typedArtifactApi.uploadAsset(id, file, fileName),
  deleteAsset: (id: string, assetId: string) => typedArtifactApi.deleteAsset(id, assetId),
  assetUrl: (id: string, assetId: string) => typedArtifactApi.assetUrl(id, assetId),

  listArtifacts: () => typedArtifactApi.listArtifacts(),

  listDocuments: () =>
    api.listArtifacts().then((artifacts) => artifacts.filter((artifact) => artifact.kind === "document")),

  getMeta: (id: string) => request<unknown>(`/api/artifacts/${id}`).then((value) => parseArtifactMeta(value, "artifact meta")),

  /** 创建指定类型的空白 Artifact；kind 是资源协议的一部分，不通过标题推断。 */
  create: (kind: ArtifactKind = "document", title?: string) =>
    request<unknown>("/api/artifacts", {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ kind, title }),
    }).then((value) => parseArtifactMeta(value, "created artifact")),

  /** 重命名或收藏。 */
  patch: (id: string, changes: { title?: string; starred?: boolean }) =>
    request<unknown>(`/api/artifacts/${id}`, {
      method: "PATCH",
      headers: { "content-type": "application/json" },
      body: JSON.stringify(changes),
    }).then((value) => parseArtifactMeta(value, "patched artifact")),

  /** 新的版本化 Artifact 快照入口；网络 JSON 先经过结构校验再交给业务层。 */
  getArtifact: async (id: string) =>
    parseSnapshot(await request<unknown>(`/api/artifacts/${id}/snapshot`)),

  getHistoryState: (id: string) =>
    request<DocumentHistoryState>(`/api/artifacts/${id}/history`),

  listSnapshots: (id: string) =>
    request<DocumentSnapshotMeta[]>(`/api/artifacts/${id}/revisions`),

  getSnapshot: async (id: string, version: number) =>
    parseSnapshot(await request<unknown>(`/api/artifacts/${id}/revisions/${version}`)),

  restoreSnapshot: (id: string, version: number, expectedVersion: number, transactionId = randomId()) =>
    request<unknown>(`/api/artifacts/${id}/revisions/${version}/restore`, {
      method: "POST",
      headers: {
        "if-match": `"${expectedVersion}"`,
        "x-transaction-id": transactionId,
      },
    }).then(parseDocumentCommitResult),

  /** 导出当前 canonical DocumentModel；原始上传文件仍由 original 端点提供。 */
  exportDocx: (id: string) => `/api/artifacts/${id}/export/docx`,
  exportPptx: (id: string) => `/api/artifacts/${id}/export/pptx`,

  submitTransaction: (id: string, transaction: ArtifactCommandEnvelope) =>
    request<unknown>(`/api/artifacts/${id}/transactions`, {
      method: "POST",
      headers: {
        "content-type": "application/json",
        "if-match": `"${transaction.baseRevision}"`,
        "x-transaction-id": transaction.transactionId,
      },
      body: JSON.stringify(transaction),
    }).then(parseDocumentCommitResult),

  /** Submit a server-authoritative undo/redo intent; no inverse document operation crosses HTTP. */
  submitHistory: (id: string, action: DocumentHistoryAction, baseRevision: number, transactionId = randomId()) =>
    request<unknown>(`/api/artifacts/${id}/transactions`, {
      method: "POST",
      headers: {
        "content-type": "application/json",
        "if-match": `"${baseRevision}"`,
        "x-transaction-id": transactionId,
      },
      body: JSON.stringify({
        protocolVersion: 1,
        transactionId,
        intentId: randomId(),
        artifactId: id,
        actorId: "dev-user",
        baseRevision,
        origin: action,
        commands: [{
          commandId: randomId(),
          typeId: DOCUMENT_HISTORY_TYPE_ID,
          payload: { action },
        }],
      } satisfies ArtifactCommandEnvelope),
    }).then(parseDocumentCommitResult),

  /** Mindmap history is also resolved on the server from durable semantic commands. */
  submitMindmapHistory: (id: string, action: DocumentHistoryAction, baseRevision: number, transactionId = randomId()) =>
    request<unknown>(`/api/artifacts/${id}/transactions`, {
      method: "POST",
      headers: {
        "content-type": "application/json",
        "if-match": `"${baseRevision}"`,
        "x-transaction-id": transactionId,
      },
      body: JSON.stringify({
        protocolVersion: 1,
        transactionId,
        intentId: randomId(),
        artifactId: id,
        actorId: "dev-user",
        baseRevision,
        origin: action,
        commands: [{ commandId: randomId(), typeId: MINDMAP_HISTORY_TYPE_ID, payload: { action } }],
      } satisfies ArtifactCommandEnvelope),
    }).then(parseDocumentCommitResult),

  upload: (file: File, mode: "audit" | "strict" = "audit") => {
    const form = new FormData();
    form.append("file", file);
    form.append("mode", mode);
    return request<unknown>("/api/artifacts/import", { method: "POST", body: form }).then(parseImportResult);
  },

  remove: (id: string) => request<void>(`/api/artifacts/${id}`, { method: "DELETE" }),
};

export interface ImportResult {
  artifact: ArtifactMeta;
  warnings: string[];
}

function parseImportResult(value: unknown): ImportResult {
  const record = asRecord(value, "imported artifact");
  if (!Array.isArray(record.warnings) || record.warnings.some((warning) => typeof warning !== "string")) {
    throw new Error("imported artifact.warnings 必须是字符串数组");
  }
  return {
    artifact: parseArtifactMeta(value, "imported artifact"),
    warnings: record.warnings as string[],
  };
}

function randomId(): string {
  return globalThis.crypto?.randomUUID?.() ?? `${Date.now()}-${Math.random().toString(16).slice(2)}`;
}

function parseDocumentCommitResult(value: unknown): DocumentCommitResult {
  const commit = parseCommitResult(value);
  const record = asRecord(value, "document commit result");
  return {
    ...commit,
    document: parseDocumentMeta(record.document),
    canUndo: asBoolean(record.canUndo, "document commit result.canUndo"),
    canRedo: asBoolean(record.canRedo, "document commit result.canRedo"),
  };
}

function parseDocumentMeta(value: unknown): DocumentMeta {
  const record = asRecord(value, "document commit result.document");
  return {
    id: asNonEmptyString(record.id, "document.id"),
    kind: asArtifactKind(record.kind, "document.kind"),
    title: asString(record.title, "document.title"),
    ownerId: asNonEmptyString(record.ownerId, "document.ownerId"),
    size: asNonNegativeInteger(record.size, "document.size"),
    version: asNonNegativeInteger(record.version, "document.version"),
    starred: asBoolean(record.starred, "document.starred"),
    createdAt: asString(record.createdAt, "document.createdAt"),
    updatedAt: asString(record.updatedAt, "document.updatedAt"),
  };
}

function asArtifactKind(value: unknown, name: string): ArtifactKind {
  if (value === "document" || value === "spreadsheet" || value === "presentation" || value === "mindmap" || value === "whiteboard") {
    return value;
  }
  throw new Error(`${name} 类型无效`);
}

function asRecord(value: unknown, name: string): Record<string, unknown> {
  if (typeof value !== "object" || value === null || Array.isArray(value)) {
    throw new Error(`${name} 必须是对象`);
  }
  return value as Record<string, unknown>;
}

function asString(value: unknown, name: string): string {
  if (typeof value !== "string") throw new Error(`${name} 必须是字符串`);
  return value;
}

function asNonEmptyString(value: unknown, name: string): string {
  const string = asString(value, name);
  if (!string.trim()) throw new Error(`${name} 不能为空`);
  return string;
}

function asNonNegativeInteger(value: unknown, name: string): number {
  if (typeof value !== "number" || !Number.isSafeInteger(value) || value < 0) {
    throw new Error(`${name} 必须是非负整数`);
  }
  return value;
}

function asBoolean(value: unknown, name: string): boolean {
  if (typeof value !== "boolean") throw new Error(`${name} 必须是布尔值`);
  return value;
}
