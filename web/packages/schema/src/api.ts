import {
  parseCellStyle,
  parseCommitResult,
  parseSpreadsheetModel,
  type ArtifactCommandEnvelope,
  type ArtifactKind,
  type ArtifactPageSetup,
  type CellStyle,
  type CommitResult,
  type DocumentBlock,
  type DocumentHeaderFooter,
  type DocumentNote,
  type DocumentPageNumbering,
  type SnapshotEnvelope,
  type SpreadsheetModel,
} from "./artifact.js";
import {
  parsePresentationV5ProjectedNode,
  parsePresentationV5Layout,
  parsePresentationV5Master,
  type PresentationV5Layout,
  type PresentationV5Master,
  type PresentationV5Node,
  type PresentationV5Slide,
  type PresentationV5SlideTransition,
  type PresentationV5TimelineEntry,
} from "./presentation-v5.js";

export type CapabilityStatus = "stable" | "preview" | "planned" | "unsupported";
export type ProjectionKind = "outline" | "tableOfContents" | "documentPrint" | "block" | "presentation" | "presentationOutline" | "presentationSlide" | "presentationNode" | "mindmap" | "whiteboard" | "spreadsheet";

export interface DocumentPrintProjection {
  revision: number;
  sections: Array<{
    sectionId: string | null;
    rootBlockIds: string[];
    pageSetup: ArtifactPageSetup | null;
    header: DocumentHeaderFooter | null;
    footer: DocumentHeaderFooter | null;
    pageNumbering: DocumentPageNumbering | null;
  }>;
  footnotes: DocumentNote[];
  endnotes: DocumentNote[];
}

export interface ArtifactMeta {
  id: string;
  kind: ArtifactKind;
  title: string;
  ownerId: string;
  size: number;
  version: number;
  starred: boolean;
  createdAt: string;
  updatedAt: string;
}

/** Immutable renderer projection for a Mindmap graph. Coordinates and routes
 * are derived by the canonical engine and are deliberately not snapshot state. */
export interface MindmapProjection {
  theme: "light" | "dark" | "highContrast";
  layout: {
    nodes: Array<{ id: string; depth: number; x: number; y: number; width: number; height: number }>;
    width: number;
    height: number;
  };
  edges: {
    routes: Array<{ edgeId: string | null; parentId: string; childId: string; points: Array<{ x: number; y: number }> }>;
  };
  advanced: {
    summaries: Array<{ summaryId: string; nodeIds: string[]; points: Array<{ x: number; y: number }>; labelAnchor: { x: number; y: number } }>;
    boundaries: Array<{ boundaryId: string; nodeIds: string[]; rect: { x: number; y: number; width: number; height: number }; labelAnchor: { x: number; y: number } }>;
    formulas: Array<{ formulaId: string; nodeId: string; anchor: { x: number; y: number } }>;
  };
}

/** Binary assets are stored beside an Artifact; snapshots retain only assetId. */
export interface ArtifactAsset {
  artifactId: string;
  assetId: string;
  objectKey: string;
  contentType: string;
  fileName: string;
  checksum: string;
  size: number;
  refCount: number;
  createdAt: string;
  updatedAt: string;
}

export interface ArtifactTransportCapability {
  snapshotEndpoint: string;
  transactionEndpoint: string;
  revisionHeader: string;
  idempotencyHeader: string;
}

export interface ArtifactCommandCapability {
  typeId: string;
  scope: string;
  requiresRevision: boolean;
  supportsIdempotency: boolean;
}

export interface ArtifactCapability {
  kind: ArtifactKind;
  namespace: string;
  features: ArtifactFeatureCapabilities;
  commands: ArtifactCommandCapability[];
}

export interface ArtifactFeatureCapabilities {
  edit: CapabilityStatus;
  history: CapabilityStatus;
  projection: CapabilityStatus;
  import: CapabilityStatus;
  export: CapabilityStatus;
  assets: CapabilityStatus;
  presence: CapabilityStatus;
}

export interface CapabilityCatalog {
  protocolVersion: number;
  contractVersion: number;
  transport: ArtifactTransportCapability;
  artifacts: ArtifactCapability[];
}

export interface ProjectionItem {
  blockId: string;
  kind: string;
  parentId: string | null;
  order: number;
  children: Array<{ blockId: string; kind: string }>;
  content?: unknown;
  headingPath?: string[];
  sourceRef?: { artifactId: string; blockId: string };
}

export interface DocumentTocItem {
  blockId: string;
  level: number;
  text: string;
}

export interface BlockProjectionItem extends ProjectionItem {
  payload?: unknown;
  refs?: CitationRef[];
}

export interface CitationRef {
  artifactId: string;
  blockId: string;
  revision: number;
  textRange: { start: number; end: number };
  headingPath: string[];
  sourceUrl?: string;
}

/** Compact, renderer-neutral facts for a v5 Presentation Deck. */
export interface PresentationDeckProjection {
  pageSpec: { width: number; height: number; unit: "emu" | "point"; safeArea?: unknown };
  themeId: string;
  themeName: string;
  slideCount: number;
  masterCount: number;
  layoutCount: number;
  assetCount: number;
  /** Read-only master metadata used to group executable slide layouts. */
  masters: PresentationMasterProjection[];
  /** Read-only layout metadata. Placeholder bodies remain in the canonical snapshot. */
  layouts: PresentationLayoutProjection[];
}

export interface PresentationMasterProjection {
  id: string;
  name: string;
  placeholderCount: number;
  /** Schema-complete read model, used only to construct typed entity commands. */
  master: PresentationV5Master;
}

export interface PresentationLayoutProjection {
  id: string;
  masterId: string;
  name: string;
  placeholderCount: number;
  /** Schema-complete read model, used only to construct typed entity commands. */
  layout: PresentationV5Layout;
}

export interface PresentationSlideOutlineItem {
  slideId: string;
  orderKey: string;
  name: string;
  layoutId: string | null;
  nodeCount: number;
  timelineEntryCount: number;
  hasNotes: boolean;
}

export interface PresentationSlideProjection {
  slideId: string;
  orderKey: string;
  name: string;
  layoutId: string | null;
  background: unknown;
  transition?: PresentationV5SlideTransition | null;
  notes?: string | null;
  nodes?: PresentationV5Node[];
  /** Present exactly when nodes are included, for strict local validation. */
  assetIds?: string[];
  timeline?: PresentationV5Slide["timeline"];
}

export interface PresentationNodeProjection {
  slideId: string;
  node: PresentationV5Node;
  childNodeIds: string[];
  assetIds: string[];
  slideNodeIds: string[];
}

export interface ProjectionEnvelope<T = unknown> {
  protocolVersion: number;
  contractVersion: number;
  artifactId: string;
  revision: number;
  projection: ProjectionKind;
  data: T;
  cursor?: string;
  nextCursor?: string;
  truncated: boolean;
}

export interface EventRecord {
  eventId: string;
  artifactId: string;
  transactionId: string;
  revision: number;
  typeId: string;
  payload: unknown;
}

export interface EventPage {
  artifactId: string;
  revision: number;
  events: EventRecord[];
  nextCursor?: string;
}

/**
 * Read-only undo/redo availability. Mutations still flow exclusively through
 * the `presentation.history` semantic command.
 */
export interface ArtifactTransactionHistoryState {
  canUndo: boolean;
  canRedo: boolean;
}

/** Short-lived collaboration projection. It is never part of a Deck snapshot. */
export interface PresentationPresenceCursor { x: number; y: number; }
export interface PresentationPresenceParticipant {
  sessionId: string;
  actorId: string;
  displayName: string;
  slideId?: string;
  selectedNodeIds: string[];
  cursor?: PresentationPresenceCursor;
}
export interface PresentationPresencePage {
  artifactId: string;
  participants: PresentationPresenceParticipant[];
  ttlMs: number;
}
export interface PresentationPresenceUpdate {
  slideId?: string;
  selectedNodeIds: string[];
  cursor?: PresentationPresenceCursor;
}

export interface DocumentPresencePoint {
  blockId: string;
  rowId?: string;
  cellId?: string;
  offset: number;
}
export interface DocumentPresenceSelection {
  anchor: DocumentPresencePoint;
  focus: DocumentPresencePoint;
}
export interface DocumentPresenceParticipant {
  sessionId: string;
  actorId: string;
  displayName: string;
  revision: number;
  blockId?: string;
  selection?: DocumentPresenceSelection;
}
export interface DocumentPresencePage {
  artifactId: string;
  participants: DocumentPresenceParticipant[];
  ttlMs: number;
}
export interface DocumentPresenceUpdate {
  revision: number;
  blockId?: string;
  selectedNodeIds: [];
  selection?: DocumentPresenceSelection;
}

export interface DocumentReviewAnchor {
  blockId: string;
  rowId?: string;
  cellId?: string;
  start: number;
  end: number;
  revision: number;
}
export interface DocumentSuggestion { originalText: string; replacement: string; }
export interface DocumentReviewMessage {
  messageId: string;
  authorId: string;
  body: string;
  mentions: string[];
  createdAt: string;
}
export interface DocumentReviewThread {
  threadId: string;
  artifactId: string;
  kind: "comment" | "suggestion";
  state: "open" | "resolved" | "accepted" | "rejected";
  authorId: string;
  anchor?: DocumentReviewAnchor;
  anchorState: "current" | "stale" | "detached";
  baseRevision: number;
  suggestion?: DocumentSuggestion;
  messages: DocumentReviewMessage[];
  createdAt: string;
  updatedAt: string;
}
export interface DocumentReviewPage {
  artifactId: string;
  revision: number;
  threads: DocumentReviewThread[];
}
export interface CreateDocumentReview {
  threadId: string;
  messageId: string;
  anchor: DocumentReviewAnchor;
  body: string;
  mentions: string[];
}
export interface CreateDocumentSuggestion extends CreateDocumentReview {
  suggestion: DocumentSuggestion;
}

/** Transport response of an accepted transaction, including history availability. */
export interface ArtifactTransactionResult extends CommitResult, ArtifactTransactionHistoryState {}

export interface ApiErrorEnvelope {
  error: string;
  code: string;
  requestId: string;
  retryable: boolean;
  details?: Record<string, unknown>;
}

export class ArtifactApiError extends Error {
  readonly status: number;
  readonly envelope: ApiErrorEnvelope | null;

  constructor(status: number, message: string, envelope: ApiErrorEnvelope | null) {
    super(message);
    this.name = "ArtifactApiError";
    this.status = status;
    this.envelope = envelope;
  }
}

export interface ArtifactApiClientOptions {
  baseUrl?: string;
  fetcher?: typeof fetch;
}

export interface ArtifactImportResult {
  artifact: ArtifactMeta;
  warnings: string[];
}

export interface ArtifactExportOptions {
  paper?: "a4" | "a3";
  orientation?: "portrait" | "landscape";
  mode?: "fit" | "tile";
  margin?: number;
}

/**
 * Framework-free REST boundary shared by the editor, SDKs and future MCP
 * adapters. It only parses transport DTOs; domain commands remain owned by
 * the corresponding engine and are never mutated here.
 */
export class ArtifactApiClient {
  private readonly baseUrl: string;
  private readonly fetcher: typeof fetch;

  constructor(options: ArtifactApiClientOptions = {}) {
    this.baseUrl = options.baseUrl ?? "";
    this.fetcher = options.fetcher ?? globalThis.fetch.bind(globalThis);
  }

  async capabilities(): Promise<CapabilityCatalog> {
    return parseCapabilityCatalog(await this.get("/api/capabilities"));
  }

  async listArtifacts(): Promise<ArtifactMeta[]> {
    const record = asRecord(await this.get("/api/artifacts"), "artifacts response");
    return asArray(record.artifacts, "artifacts response.artifacts").map((item, index) =>
      parseArtifactMeta(item, `artifacts[${index}]`),
    );
  }

  async importArtifact(file: Blob, fileName: string, mode: "audit" | "strict" = "audit"): Promise<ArtifactImportResult> {
    const form = new FormData();
    form.append("file", file, fileName);
    form.append("mode", mode);
    const value = asRecord(await this.request("/api/artifacts/import", { method: "POST", body: form }), "imported artifact");
    const warnings = asArray(value.warnings, "imported artifact.warnings").map((warning, index) => asString(warning, `imported artifact.warnings[${index}]`));
    return { artifact: parseArtifactMeta(value, "imported artifact"), warnings };
  }

  async exportArtifact(artifactId: string, format: "json" | "md" | "svg" | "pdf", options: ArtifactExportOptions = {}): Promise<Blob> {
    const query = new URLSearchParams();
    if (options.paper) query.set("paper", options.paper);
    if (options.orientation) query.set("orientation", options.orientation);
    if (options.mode) query.set("mode", options.mode);
    if (options.margin !== undefined) query.set("margin", String(options.margin));
    const suffix = query.size ? `?${query}` : "";
    const response = await this.fetcher(`${this.baseUrl}/api/artifacts/${encodeURIComponent(artifactId)}/export/${format}${suffix}`, { method: "GET", cache: "no-store" });
    if (!response.ok) await throwApiError(response);
    return response.blob();
  }

  async outline(
    artifactId: string,
    options: ProjectionRequest = {},
  ): Promise<ProjectionEnvelope<{ items: ProjectionItem[] }>> {
    const envelope = parseProjectionEnvelope(await this.getProjection(artifactId, "outline", options), "outline");
    return { ...envelope, data: parseProjectionItems(envelope.data, "outline.data") };
  }

  async tableOfContents(
    artifactId: string,
    options: ProjectionRequest = {},
  ): Promise<ProjectionEnvelope<{ items: DocumentTocItem[] }>> {
    const envelope = parseProjectionEnvelope(
      await this.getProjection(artifactId, "toc", options),
      "tableOfContents",
    );
    const data = asRecord(envelope.data, "toc.data");
    const items = asArray(data.items, "toc.data.items").map((value, index) => {
      const item = asRecord(value, `toc.data.items[${index}]`);
      const level = asNonNegativeInteger(item.level, `toc.data.items[${index}].level`);
      if (level < 1 || level > 6) throw new Error(`toc.data.items[${index}].level 无效`);
      return {
        blockId: asNonEmptyString(item.blockId, `toc.data.items[${index}].blockId`),
        level,
        text: asString(item.text, `toc.data.items[${index}].text`),
      };
    });
    return { ...envelope, data: { items } };
  }

  async documentPrint(artifactId: string): Promise<ProjectionEnvelope<DocumentPrintProjection>> {
    const envelope = parseProjectionEnvelope(
      await this.getProjection(artifactId, "projection/documentPrint", {}),
      "documentPrint",
    );
    return { ...envelope, data: parseDocumentPrintProjection(envelope.data) };
  }

  async blocks(
    artifactId: string,
    options: ProjectionRequest = {},
  ): Promise<ProjectionEnvelope<{ items: BlockProjectionItem[] }>> {
    const envelope = parseProjectionEnvelope(await this.getProjection(artifactId, "blocks", options), "block");
    return { ...envelope, data: parseBlockProjectionItems(envelope.data, "blocks.data") };
  }

  async block(artifactId: string, blockId: string, options: ProjectionRequest = {}): Promise<ProjectionEnvelope<BlockProjectionItem>> {
    const envelope = parseProjectionEnvelope(await this.getProjection(artifactId, `blocks/${encodeURIComponent(blockId)}`, options), "block");
    return { ...envelope, data: parseProjectionItem(envelope.data, "block.data") };
  }

  /** Read compact Deck facts only; this never enables Presentation writes. */
  async presentation(artifactId: string): Promise<ProjectionEnvelope<PresentationDeckProjection>> {
    const envelope = parseProjectionEnvelope(await this.getProjection(artifactId, "projection/presentation", {}), "presentation");
    return { ...envelope, data: parsePresentationDeckProjection(envelope.data) };
  }

  /** Read a renderer-only Mindmap layout. Writes remain semantic transactions. */
  async mindmap(artifactId: string, theme: MindmapProjection["theme"] = "light"): Promise<ProjectionEnvelope<MindmapProjection>> {
    const envelope = parseProjectionEnvelope(
      await this.get(`/api/artifacts/${encodeURIComponent(artifactId)}/projection/mindmap?theme=${encodeURIComponent(theme)}`),
      "mindmap",
    );
    return { ...envelope, data: parseMindmapProjection(envelope.data) };
  }

  /** Cursor- and budget-bounded slide outline for agents, indexers and navigators. */
  async presentationOutline(
    artifactId: string,
    options: PresentationOutlineRequest = {},
  ): Promise<ProjectionEnvelope<{ items: PresentationSlideOutlineItem[] }>> {
    const envelope = parseProjectionEnvelope(await this.getPresentationProjection(artifactId, "outline", options), "presentationOutline");
    return { ...envelope, data: parsePresentationOutline(envelope.data) };
  }

  /** Read one slide; large sections require an explicit include list. */
  async presentationSlide(
    artifactId: string,
    slideId: string,
    options: PresentationSlideRequest = {},
  ): Promise<ProjectionEnvelope<PresentationSlideProjection>> {
    const envelope = parseProjectionEnvelope(
      await this.getPresentationProjection(artifactId, `slides/${encodeURIComponent(slideId)}`, options),
      "presentationSlide",
    );
    return { ...envelope, data: parsePresentationSlideProjection(envelope.data) };
  }

  /** Read one slide-scoped node; bare node ids are intentionally unsupported. */
  async presentationNode(
    artifactId: string,
    slideId: string,
    nodeId: string,
    options: PresentationNodeRequest = {},
  ): Promise<ProjectionEnvelope<PresentationNodeProjection>> {
    const envelope = parseProjectionEnvelope(
      await this.getPresentationProjection(
        artifactId,
        `slides/${encodeURIComponent(slideId)}/nodes/${encodeURIComponent(nodeId)}`,
        options,
      ),
      "presentationNode",
    );
    return { ...envelope, data: parsePresentationNodeProjection(envelope.data) };
  }

  /** Read a bounded, read-only spreadsheet grid window. This never enables
   * spreadsheet writes; edits must use the advertised semantic commands. */
  async spreadsheet(
    artifactId: string,
    options: SpreadsheetProjectionRequest,
  ): Promise<ProjectionEnvelope<SpreadsheetGridProjection>> {
    const query = new URLSearchParams();
    query.set("sheetId", options.sheetId);
    query.set("startRow", String(options.startRow));
    query.set("endRow", String(options.endRow));
    query.set("startColumn", String(options.startColumn));
    query.set("endColumn", String(options.endColumn));
    if (options.maxBytes !== undefined) query.set("maxBytes", String(options.maxBytes));
    const suffix = query.toString() ? `?${query}` : "";
    const envelope = parseProjectionEnvelope(
      await this.get(`/api/artifacts/${encodeURIComponent(artifactId)}/projection/spreadsheet${suffix}`),
      "spreadsheet",
    );
    return { ...envelope, data: parseSpreadsheetGridProjection(envelope.data) };
  }

  async spreadsheetStructure(
    artifactId: string,
  ): Promise<ProjectionEnvelope<SpreadsheetStructureProjection>> {
    const envelope = parseProjectionEnvelope(
      await this.get(`/api/artifacts/${encodeURIComponent(artifactId)}/projection/spreadsheet`),
      "spreadsheet",
    );
    const data = parseSpreadsheetStructureProjection(envelope.data);
    return { ...envelope, data };
  }

  /** Read compact undo/redo availability without exposing an editable log. */
  async history(artifactId: string): Promise<ArtifactTransactionHistoryState> {
    return parseArtifactTransactionHistoryState(
      await this.get(`/api/artifacts/${encodeURIComponent(artifactId)}/history`),
    );
  }

  /**
   * Read live scene/graph collaborators. Presence is intentionally separate
   * from events and transactions: readers must never use it for recovery.
   */
  async presentationPresence(artifactId: string): Promise<PresentationPresencePage> {
    return parsePresentationPresencePage(
      await this.get(`/api/artifacts/${encodeURIComponent(artifactId)}/presence`),
    );
  }

  /** Refresh this browser session's ephemeral view projection. */
  async updatePresentationPresence(
    artifactId: string,
    sessionId: string,
    update: PresentationPresenceUpdate,
  ): Promise<void> {
    assertPresenceSessionId(sessionId);
    await this.request(`/api/artifacts/${encodeURIComponent(artifactId)}/presence/${encodeURIComponent(sessionId)}`, {
      method: "PUT",
      headers: { "content-type": "application/json" },
      body: JSON.stringify(update),
    });
  }

  async documentPresence(artifactId: string): Promise<DocumentPresencePage> {
    return parseDocumentPresencePage(
      await this.get(`/api/artifacts/${encodeURIComponent(artifactId)}/presence`),
    );
  }

  async updateDocumentPresence(
    artifactId: string,
    sessionId: string,
    update: DocumentPresenceUpdate,
  ): Promise<void> {
    assertPresenceSessionId(sessionId);
    await this.request(`/api/artifacts/${encodeURIComponent(artifactId)}/presence/${encodeURIComponent(sessionId)}`, {
      method: "PUT",
      headers: { "content-type": "application/json" },
      body: JSON.stringify(update),
    });
  }

  async documentReviews(artifactId: string): Promise<DocumentReviewPage> {
    return parseDocumentReviewPage(
      await this.get(`/api/artifacts/${encodeURIComponent(artifactId)}/reviews`),
    );
  }

  async createDocumentReview(artifactId: string, review: CreateDocumentReview): Promise<DocumentReviewPage> {
    return parseDocumentReviewPage(await this.request(`/api/artifacts/${encodeURIComponent(artifactId)}/reviews`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify(review),
    }));
  }

  async createDocumentSuggestion(artifactId: string, suggestion: CreateDocumentSuggestion): Promise<DocumentReviewPage> {
    return parseDocumentReviewPage(await this.request(`/api/artifacts/${encodeURIComponent(artifactId)}/suggestions`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify(suggestion),
    }));
  }

  async replyDocumentReview(
    artifactId: string,
    threadId: string,
    message: { messageId: string; body: string; mentions: string[] },
  ): Promise<void> {
    await this.request(`/api/artifacts/${encodeURIComponent(artifactId)}/reviews/${encodeURIComponent(threadId)}/messages`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify(message),
    });
  }

  async updateDocumentReview(
    artifactId: string,
    threadId: string,
    state: DocumentReviewThread["state"],
  ): Promise<void> {
    await this.request(`/api/artifacts/${encodeURIComponent(artifactId)}/reviews/${encodeURIComponent(threadId)}`, {
      method: "PATCH",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ state }),
    });
  }

  async events(artifactId: string, options: EventRequest = {}): Promise<EventPage> {
    const query = new URLSearchParams();
    if (options.sinceRevision !== undefined) query.set("sinceRevision", String(options.sinceRevision));
    if (options.cursor) query.set("cursor", options.cursor);
    if (options.limit !== undefined) query.set("limit", String(options.limit));
    const suffix = query.toString() ? `?${query}` : "";
    return parseEventPage(await this.get(`/api/artifacts/${encodeURIComponent(artifactId)}/events${suffix}`));
  }

  /** Read command-history availability without exposing an editable history model. */
  async uploadAsset(artifactId: string, file: Blob, fileName = "image.png"): Promise<ArtifactAsset> {
    const form = new FormData();
    form.append("file", file, fileName);
    return parseArtifactAsset(await this.request(`/api/artifacts/${encodeURIComponent(artifactId)}/assets`, {
      method: "POST",
      body: form,
    }));
  }

  async deleteAsset(artifactId: string, assetId: string): Promise<void> {
    await this.request(`/api/artifacts/${encodeURIComponent(artifactId)}/assets/${encodeURIComponent(assetId)}`, {
      method: "DELETE",
    });
  }

  assetUrl(artifactId: string, assetId: string): string {
    return `${this.baseUrl}/api/artifacts/${encodeURIComponent(artifactId)}/assets/${encodeURIComponent(assetId)}`;
  }

  async submitTransaction(
    transaction: ArtifactCommandEnvelope,
  ): Promise<ArtifactTransactionResult> {
    const value = await this.request(`/api/artifacts/${encodeURIComponent(transaction.artifactId)}/transactions`, {
      method: "POST",
      headers: {
        "content-type": "application/json",
        "if-match": `"${transaction.baseRevision}"`,
        "x-transaction-id": transaction.transactionId,
      },
      body: JSON.stringify(transaction),
    });
    return parseArtifactTransactionResult(value);
  }

  private async getProjection(artifactId: string, path: string, options: ProjectionRequest): Promise<unknown> {
    const query = new URLSearchParams();
    if (options.include?.length) query.set("include", options.include.join(","));
    if (options.parentId) query.set("parentId", options.parentId);
    if (options.cursor) query.set("cursor", options.cursor);
    if (options.limit !== undefined) query.set("limit", String(options.limit));
    if (options.maxBytes !== undefined) query.set("maxBytes", String(options.maxBytes));
    const suffix = query.toString() ? `?${query}` : "";
    return this.get(`/api/artifacts/${encodeURIComponent(artifactId)}/${path}${suffix}`);
  }

  private async getPresentationProjection(
    artifactId: string,
    path: string,
    options: PresentationProjectionRequest,
  ): Promise<unknown> {
    const query = new URLSearchParams();
    if (options.include?.length) query.set("include", options.include.join(","));
    if (options.cursor) query.set("cursor", options.cursor);
    if (options.limit !== undefined) query.set("limit", String(options.limit));
    if (options.maxBytes !== undefined) query.set("maxBytes", String(options.maxBytes));
    const suffix = query.toString() ? `?${query}` : "";
    return this.get(`/api/artifacts/${encodeURIComponent(artifactId)}/presentation/${path}${suffix}`);
  }

  private async get(path: string): Promise<unknown> {
    // Artifact projections are mutable while retaining stable URLs. Never let a
    // browser reuse a previous revision here: callers already receive ETag and
    // revision fields, and a stale deck can otherwise fail strict decoding after
    // a schema or projection upgrade.
    return this.request(path, { method: "GET", cache: "no-store" });
  }

  private async request(path: string, init: RequestInit): Promise<unknown> {
    const response = await this.fetcher(`${this.baseUrl}${path}`, init);
    if (response.ok) {
      if (response.status === 204) return undefined;
      return response.json() as Promise<unknown>;
    }
    return throwApiError(response);
  }
}

async function throwApiError(response: Response): Promise<never> {
  let envelope: ApiErrorEnvelope | null = null;
  try {
    envelope = parseApiError(await response.json());
  } catch {
    // Keep a transport error even if a proxy returned non-JSON content.
  }
  throw new ArtifactApiError(response.status, envelope?.error ?? `HTTP ${response.status}`, envelope);
}

export interface ProjectionRequest {
  include?: Array<"content" | "headingPath" | "refs">;
  parentId?: string;
  cursor?: string;
  limit?: number;
  maxBytes?: number;
}

export interface PresentationOutlineRequest {
  cursor?: string;
  limit?: number;
  maxBytes?: number;
}

export interface PresentationSlideRequest {
  include?: Array<"nodes" | "notes" | "timeline">;
  maxBytes?: number;
}

export interface PresentationNodeRequest {
  maxBytes?: number;
}

/** Bounded spreadsheet grid window. The window is a read-only request; only
 * materialized cells in the half-open range are returned, so a million-row sheet
 * never materializes a million render nodes in the browser. */
export interface SpreadsheetProjectionRequest {
  sheetId: string;
  /** Half-open viewport `[start, end)`, 1-based row/column are 0-based here. */
  startRow: number;
  endRow: number;
  startColumn: number;
  endColumn: number;
  maxBytes?: number;
}

/** One materialized cell inside a grid window. `value`/`formula`/`style` mirror
 * the canonical sparse snapshot; this is a read projection, never an editable model. */
export interface SpreadsheetGridCell {
  address: { sheetId: string; row: number; column: number };
  value?: unknown;
  formula?: string;
  attrs: Record<string, unknown>;
  style?: CellStyle | null;
}

/** Derived formula value computed by the canonical Rust calculator. The browser
 * displays it; it never writes a result back into a persisted cell. */
export type SpreadsheetCalculatedValue =
  | { type: "blank" }
  | { type: "number"; value: number }
  | { type: "text"; value: string }
  | { type: "bool"; value: boolean }
  | { type: "error"; value: { code: string; message: string } };

export interface SpreadsheetGridProjection {
  sheetId: string;
  startRow: number;
  endRow: number;
  startColumn: number;
  endColumn: number;
  cells: SpreadsheetGridCell[];
  cellCount: number;
  sparse: boolean;
  /** keyed by `"row:column"` for addresses inside the window. */
  values: Record<string, SpreadsheetCalculatedValue>;
  /** 条件格式命中：`"row:column"` -> 命中的规则 id 列表（CellIs 服务端求值）。 */
  conditionalStyles: Record<string, string[]>;
  /** 被筛选谓词排除的行号（窗口内）。 */
  filteredOutRows: number[];
}

/** Workbook/sheet structure without sparse cell payloads. */
export type SpreadsheetStructureProjection = SpreadsheetModel;

export function parseSpreadsheetStructureProjection(value: unknown): SpreadsheetStructureProjection {
  const data = parseSpreadsheetModel(value);
  if (data.sheets.some((sheet) => sheet.cells.length > 0)) {
    throw new Error("spreadsheet structure projection 不得包含 cell payload");
  }
  return data;
}

export function parsePresentationPresencePage(value: unknown): PresentationPresencePage {
  const record = asRecord(value, "presentation presence");
  const artifactId = asString(record.artifactId, "presence.artifactId");
  const ttlMs = asNonNegativeInteger(record.ttlMs, "presence.ttlMs");
  const participants = asArray(record.participants, "presence.participants").map((item, index) => {
    const participant = asRecord(item, `presence.participants[${index}]`);
    const selectedNodeIds = asArray(participant.selectedNodeIds, `presence.participants[${index}].selectedNodeIds`)
      .map((nodeId, nodeIndex) => asString(nodeId, `presence.participants[${index}].selectedNodeIds[${nodeIndex}]`));
    const cursorValue = participant.cursor;
    const cursor = cursorValue === undefined || cursorValue === null ? undefined : (() => {
      const cursorRecord = asRecord(cursorValue, `presence.participants[${index}].cursor`);
      return { x: asFiniteNumber(cursorRecord.x, `presence.participants[${index}].cursor.x`), y: asFiniteNumber(cursorRecord.y, `presence.participants[${index}].cursor.y`) };
    })();
    return {
      sessionId: asString(participant.sessionId, `presence.participants[${index}].sessionId`),
      actorId: asString(participant.actorId, `presence.participants[${index}].actorId`),
      displayName: asString(participant.displayName, `presence.participants[${index}].displayName`),
      ...(participant.slideId === undefined || participant.slideId === null ? {} : { slideId: asString(participant.slideId, `presence.participants[${index}].slideId`) }),
      selectedNodeIds,
      ...(cursor === undefined ? {} : { cursor }),
    };
  });
  return { artifactId, ttlMs, participants };
}

export function parseDocumentPresencePage(value: unknown): DocumentPresencePage {
  const record = asRecord(value, "document presence");
  const participants = asArray(record.participants, "presence.participants").map((value, index) => {
    const item = asRecord(value, `presence.participants[${index}]`);
    const selection = item.selection === undefined || item.selection === null
      ? undefined
      : parseDocumentPresenceSelection(item.selection, `presence.participants[${index}].selection`);
    return {
      sessionId: asString(item.sessionId, `presence.participants[${index}].sessionId`),
      actorId: asString(item.actorId, `presence.participants[${index}].actorId`),
      displayName: asString(item.displayName, `presence.participants[${index}].displayName`),
      revision: asNonNegativeInteger(item.revision, `presence.participants[${index}].revision`),
      ...(item.blockId === undefined || item.blockId === null ? {} : { blockId: asString(item.blockId, `presence.participants[${index}].blockId`) }),
      ...(selection ? { selection } : {}),
    };
  });
  return {
    artifactId: asString(record.artifactId, "presence.artifactId"),
    ttlMs: asNonNegativeInteger(record.ttlMs, "presence.ttlMs"),
    participants,
  };
}

function parseDocumentPresenceSelection(value: unknown, label: string): DocumentPresenceSelection {
  const record = asRecord(value, label);
  return {
    anchor: parseDocumentPresencePoint(record.anchor, `${label}.anchor`),
    focus: parseDocumentPresencePoint(record.focus, `${label}.focus`),
  };
}

function parseDocumentPresencePoint(value: unknown, label: string): DocumentPresencePoint {
  const record = asRecord(value, label);
  return {
    blockId: asString(record.blockId, `${label}.blockId`),
    ...(record.rowId === undefined || record.rowId === null ? {} : { rowId: asString(record.rowId, `${label}.rowId`) }),
    ...(record.cellId === undefined || record.cellId === null ? {} : { cellId: asString(record.cellId, `${label}.cellId`) }),
    offset: asNonNegativeInteger(record.offset, `${label}.offset`),
  };
}

export function parseDocumentReviewPage(value: unknown): DocumentReviewPage {
  const record = asRecord(value, "document reviews");
  return {
    artifactId: asString(record.artifactId, "reviews.artifactId"),
    revision: asNonNegativeInteger(record.revision, "reviews.revision"),
    threads: asArray(record.threads, "reviews.threads").map(parseDocumentReviewThread),
  };
}

function parseDocumentReviewThread(value: unknown, index: number): DocumentReviewThread {
  const label = `reviews.threads[${index}]`;
  const record = asRecord(value, label);
  const kind = asString(record.kind, `${label}.kind`);
  const state = asString(record.state, `${label}.state`);
  const anchorState = asString(record.anchorState, `${label}.anchorState`);
  if (kind !== "comment" && kind !== "suggestion") throw new Error(`${label}.kind 无效`);
  if (!["open", "resolved", "accepted", "rejected"].includes(state)) throw new Error(`${label}.state 无效`);
  if (!["current", "stale", "detached"].includes(anchorState)) throw new Error(`${label}.anchorState 无效`);
  const anchor = record.anchor === undefined || record.anchor === null ? undefined : parseDocumentReviewAnchor(record.anchor, `${label}.anchor`);
  const suggestion = record.suggestion === undefined || record.suggestion === null ? undefined : (() => {
    const item = asRecord(record.suggestion, `${label}.suggestion`);
    return { originalText: asString(item.originalText, `${label}.suggestion.originalText`), replacement: asString(item.replacement, `${label}.suggestion.replacement`) };
  })();
  return {
    threadId: asString(record.threadId, `${label}.threadId`),
    artifactId: asString(record.artifactId, `${label}.artifactId`),
    kind,
    state: state as DocumentReviewThread["state"],
    authorId: asString(record.authorId, `${label}.authorId`),
    ...(anchor ? { anchor } : {}),
    anchorState: anchorState as DocumentReviewThread["anchorState"],
    baseRevision: asNonNegativeInteger(record.baseRevision, `${label}.baseRevision`),
    ...(suggestion ? { suggestion } : {}),
    messages: asArray(record.messages, `${label}.messages`).map((message, messageIndex) => {
      const item = asRecord(message, `${label}.messages[${messageIndex}]`);
      return {
        messageId: asString(item.messageId, `${label}.messages[${messageIndex}].messageId`),
        authorId: asString(item.authorId, `${label}.messages[${messageIndex}].authorId`),
        body: asString(item.body, `${label}.messages[${messageIndex}].body`),
        mentions: asArray(item.mentions, `${label}.messages[${messageIndex}].mentions`).map((mention, mentionIndex) => asString(mention, `${label}.messages[${messageIndex}].mentions[${mentionIndex}]`)),
        createdAt: asString(item.createdAt, `${label}.messages[${messageIndex}].createdAt`),
      };
    }),
    createdAt: asString(record.createdAt, `${label}.createdAt`),
    updatedAt: asString(record.updatedAt, `${label}.updatedAt`),
  };
}

function parseDocumentReviewAnchor(value: unknown, label: string): DocumentReviewAnchor {
  const record = asRecord(value, label);
  return {
    blockId: asString(record.blockId, `${label}.blockId`),
    ...(record.rowId === undefined || record.rowId === null ? {} : { rowId: asString(record.rowId, `${label}.rowId`) }),
    ...(record.cellId === undefined || record.cellId === null ? {} : { cellId: asString(record.cellId, `${label}.cellId`) }),
    start: asNonNegativeInteger(record.start, `${label}.start`),
    end: asNonNegativeInteger(record.end, `${label}.end`),
    revision: asNonNegativeInteger(record.revision, `${label}.revision`),
  };
}

function assertPresenceSessionId(value: string): void {
  if (!/^[A-Za-z0-9_-]{1,128}$/.test(value)) throw new Error("presence sessionId 非法");
}

type PresentationProjectionRequest = {
  include?: Array<"nodes" | "notes" | "timeline">;
  cursor?: string;
  limit?: number;
  maxBytes?: number;
};

export interface EventRequest {
  sinceRevision?: number;
  cursor?: string;
  limit?: number;
}

export function parseArtifactAsset(value: unknown): ArtifactAsset {
  const record = asRecord(value, "artifact asset");
  return {
    artifactId: asNonEmptyString(record.artifactId, "artifact asset.artifactId"),
    assetId: asNonEmptyString(record.assetId, "artifact asset.assetId"),
    objectKey: asNonEmptyString(record.objectKey, "artifact asset.objectKey"),
    contentType: asNonEmptyString(record.contentType, "artifact asset.contentType"),
    fileName: asNonEmptyString(record.fileName, "artifact asset.fileName"),
    checksum: asNonEmptyString(record.checksum, "artifact asset.checksum"),
    size: asNonNegativeInteger(record.size, "artifact asset.size"),
    refCount: asNonNegativeInteger(record.refCount, "artifact asset.refCount"),
    createdAt: asNonEmptyString(record.createdAt, "artifact asset.createdAt"),
    updatedAt: asNonEmptyString(record.updatedAt, "artifact asset.updatedAt"),
  };
}

export function parseCapabilityCatalog(value: unknown): CapabilityCatalog {
  const record = asRecord(value, "capability catalog");
  const transport = asRecord(record.transport, "capability catalog.transport");
  const artifacts = asArray(record.artifacts, "capability catalog.artifacts").map((item, index) => {
    const artifact = asRecord(item, `capability catalog.artifacts[${index}]`);
    const features = asRecord(artifact.features, `capability catalog.artifacts[${index}].features`);
    return {
      kind: asArtifactKind(artifact.kind, "capability kind"),
      namespace: asNonEmptyString(artifact.namespace, "capability namespace"),
      features: {
        edit: asCapabilityStatus(features.edit, "capability features.edit"),
        history: asCapabilityStatus(features.history, "capability features.history"),
        projection: asCapabilityStatus(features.projection, "capability features.projection"),
        import: asCapabilityStatus(features.import, "capability features.import"),
        export: asCapabilityStatus(features.export, "capability features.export"),
        assets: asCapabilityStatus(features.assets, "capability features.assets"),
        presence: asCapabilityStatus(features.presence, "capability features.presence"),
      },
      commands: asArray(artifact.commands, "capability commands").map((command, commandIndex) => {
        const item = asRecord(command, `capability command[${commandIndex}]`);
        return {
          typeId: asNonEmptyString(item.typeId, "capability command.typeId"),
          scope: asNonEmptyString(item.scope, "capability command.scope"),
          requiresRevision: asBoolean(item.requiresRevision, "capability command.requiresRevision"),
          supportsIdempotency: asBoolean(item.supportsIdempotency, "capability command.supportsIdempotency"),
        };
      }),
    };
  });
  return {
    protocolVersion: asPositiveInteger(record.protocolVersion, "capability catalog.protocolVersion"),
    contractVersion: asPositiveInteger(record.contractVersion, "capability catalog.contractVersion"),
    transport: {
      snapshotEndpoint: asNonEmptyString(transport.snapshotEndpoint, "transport.snapshotEndpoint"),
      transactionEndpoint: asNonEmptyString(transport.transactionEndpoint, "transport.transactionEndpoint"),
      revisionHeader: asNonEmptyString(transport.revisionHeader, "transport.revisionHeader"),
      idempotencyHeader: asNonEmptyString(transport.idempotencyHeader, "transport.idempotencyHeader"),
    },
    artifacts,
  };
}

export function parseProjectionEnvelope<T = unknown>(value: unknown, expectedProjection?: ProjectionKind): ProjectionEnvelope<T> {
  const record = asRecord(value, "projection envelope");
  const projection = asProjectionKind(record.projection, "projection envelope.projection");
  if (expectedProjection && projection !== expectedProjection) throw new Error("projection 类型与请求不一致");
  return {
    protocolVersion: asPositiveInteger(record.protocolVersion, "projection.protocolVersion"),
    contractVersion: asPositiveInteger(record.contractVersion, "projection.contractVersion"),
    artifactId: asNonEmptyString(record.artifactId, "projection.artifactId"),
    revision: asNonNegativeInteger(record.revision, "projection.revision"),
    projection,
    data: asRecord(record.data, "projection.data") as T,
    ...(record.cursor === undefined ? {} : { cursor: asString(record.cursor, "projection.cursor") }),
    ...(record.nextCursor === undefined ? {} : { nextCursor: asString(record.nextCursor, "projection.nextCursor") }),
    truncated: asBoolean(record.truncated, "projection.truncated"),
  };
}

export function parseDocumentPrintProjection(value: unknown): DocumentPrintProjection {
  const record = asRecord(value, "documentPrint.data");
  const sections = asArray(record.sections, "documentPrint.data.sections").map((value, index) => {
    const section = asRecord(value, `documentPrint.data.sections[${index}]`);
    const sectionId = section.sectionId === null ? null : asNonEmptyString(section.sectionId, `documentPrint.data.sections[${index}].sectionId`);
    const rootBlockIds = asArray(section.rootBlockIds, `documentPrint.data.sections[${index}].rootBlockIds`).map((id, blockIndex) =>
      asNonEmptyString(id, `documentPrint.data.sections[${index}].rootBlockIds[${blockIndex}]`),
    );
    const nullableRecord = <T>(field: unknown, name: string): T | null => field === null ? null : asRecord(field, name) as T;
    return {
      sectionId,
      rootBlockIds,
      pageSetup: nullableRecord<ArtifactPageSetup>(section.pageSetup, `documentPrint.data.sections[${index}].pageSetup`),
      header: nullableRecord<DocumentHeaderFooter>(section.header, `documentPrint.data.sections[${index}].header`),
      footer: nullableRecord<DocumentHeaderFooter>(section.footer, `documentPrint.data.sections[${index}].footer`),
      pageNumbering: nullableRecord<DocumentPageNumbering>(section.pageNumbering, `documentPrint.data.sections[${index}].pageNumbering`),
    };
  });
  const parseNotes = (value: unknown, name: string): DocumentNote[] => asArray(value, name).map((item, index) => {
    const note = asRecord(item, `${name}[${index}]`);
    const anchor = asRecord(note.anchor, `${name}[${index}].anchor`);
    asNonEmptyString(note.id, `${name}[${index}].id`);
    asNonEmptyString(anchor.blockId, `${name}[${index}].anchor.blockId`);
    asNonNegativeInteger(anchor.start, `${name}[${index}].anchor.start`);
    asNonNegativeInteger(anchor.end, `${name}[${index}].anchor.end`);
    asArray(note.content, `${name}[${index}].content`);
    return item as DocumentNote;
  });
  return {
    revision: asNonNegativeInteger(record.revision, "documentPrint.data.revision"),
    sections,
    footnotes: parseNotes(record.footnotes, "documentPrint.data.footnotes"),
    endnotes: parseNotes(record.endnotes, "documentPrint.data.endnotes"),
  };
}

export function parseMindmapProjection(value: unknown): MindmapProjection {
  const record = asRecord(value, "mindmap projection data");
  const theme = record.theme;
  if (theme !== "light" && theme !== "dark" && theme !== "highContrast") {
    throw new Error("mindmap projection theme 无效");
  }
  const layout = asRecord(record.layout, "mindmap projection layout");
  const edges = asRecord(record.edges, "mindmap projection edges");
  const advanced = asRecord(record.advanced, "mindmap projection advanced");
  const point = (value: unknown, name: string) => {
    const item = asRecord(value, name);
    return { x: asFiniteNumber(item.x, `${name}.x`), y: asFiniteNumber(item.y, `${name}.y`) };
  };
  return {
    theme,
    layout: {
      width: asFiniteNumber(layout.width, "mindmap layout.width"),
      height: asFiniteNumber(layout.height, "mindmap layout.height"),
      nodes: asArray(layout.nodes, "mindmap layout.nodes").map((raw, index) => {
        const node = asRecord(raw, `mindmap layout.nodes[${index}]`);
        return {
          id: asNonEmptyString(node.id, `mindmap layout.nodes[${index}].id`),
          depth: asNonNegativeInteger(node.depth, `mindmap layout.nodes[${index}].depth`),
          x: asFiniteNumber(node.x, `mindmap layout.nodes[${index}].x`),
          y: asFiniteNumber(node.y, `mindmap layout.nodes[${index}].y`),
          width: asFiniteNumber(node.width, `mindmap layout.nodes[${index}].width`),
          height: asFiniteNumber(node.height, `mindmap layout.nodes[${index}].height`),
        };
      }),
    },
    edges: {
      routes: asArray(edges.routes, "mindmap edges.routes").map((raw, index) => {
        const route = asRecord(raw, `mindmap edges.routes[${index}]`);
        const edgeId = route.edgeId;
        if (edgeId !== null && edgeId !== undefined && typeof edgeId !== "string") {
          throw new Error(`mindmap edges.routes[${index}].edgeId 无效`);
        }
        return {
          edgeId: edgeId ?? null,
          parentId: asNonEmptyString(route.parentId, `mindmap edges.routes[${index}].parentId`),
          childId: asNonEmptyString(route.childId, `mindmap edges.routes[${index}].childId`),
          points: asArray(route.points, `mindmap edges.routes[${index}].points`).map((point, pointIndex) => {
            const value = asRecord(point, `mindmap edges.routes[${index}].points[${pointIndex}]`);
            return { x: asFiniteNumber(value.x, "mindmap point.x"), y: asFiniteNumber(value.y, "mindmap point.y") };
          }),
        };
      }),
    },
    advanced: {
      summaries: asArray(advanced.summaries, "mindmap advanced.summaries").map((raw, index) => {
        const summary = asRecord(raw, `mindmap advanced.summaries[${index}]`);
        return {
          summaryId: asNonEmptyString(summary.summaryId, `mindmap advanced.summaries[${index}].summaryId`),
          nodeIds: asNonEmptyStringArray(summary.nodeIds, `mindmap advanced.summaries[${index}].nodeIds`),
          points: asArray(summary.points, `mindmap advanced.summaries[${index}].points`).map((item, pointIndex) => point(item, `mindmap advanced.summaries[${index}].points[${pointIndex}]`)),
          labelAnchor: point(summary.labelAnchor, `mindmap advanced.summaries[${index}].labelAnchor`),
        };
      }),
      boundaries: asArray(advanced.boundaries, "mindmap advanced.boundaries").map((raw, index) => {
        const boundary = asRecord(raw, `mindmap advanced.boundaries[${index}]`);
        const rect = asRecord(boundary.rect, `mindmap advanced.boundaries[${index}].rect`);
        return {
          boundaryId: asNonEmptyString(boundary.boundaryId, `mindmap advanced.boundaries[${index}].boundaryId`),
          nodeIds: asNonEmptyStringArray(boundary.nodeIds, `mindmap advanced.boundaries[${index}].nodeIds`),
          rect: { x: asFiniteNumber(rect.x, "mindmap boundary rect.x"), y: asFiniteNumber(rect.y, "mindmap boundary rect.y"), width: asFiniteNumber(rect.width, "mindmap boundary rect.width"), height: asFiniteNumber(rect.height, "mindmap boundary rect.height") },
          labelAnchor: point(boundary.labelAnchor, `mindmap advanced.boundaries[${index}].labelAnchor`),
        };
      }),
      formulas: asArray(advanced.formulas, "mindmap advanced.formulas").map((raw, index) => {
        const formula = asRecord(raw, `mindmap advanced.formulas[${index}]`);
        return { formulaId: asNonEmptyString(formula.formulaId, `mindmap advanced.formulas[${index}].formulaId`), nodeId: asNonEmptyString(formula.nodeId, `mindmap advanced.formulas[${index}].nodeId`), anchor: point(formula.anchor, `mindmap advanced.formulas[${index}].anchor`) };
      }),
    },
  };
}

export function parseSpreadsheetGridProjection(value: unknown): SpreadsheetGridProjection {
  const record = asRecord(value, "spreadsheet projection data");
  const sheetId = asNonEmptyString(record.sheetId, "spreadsheet data.sheetId");
  const startRow = asNonNegativeInteger(record.startRow, "spreadsheet data.startRow");
  const endRow = asNonNegativeInteger(record.endRow, "spreadsheet data.endRow");
  const startColumn = asNonNegativeInteger(record.startColumn, "spreadsheet data.startColumn");
  const endColumn = asNonNegativeInteger(record.endColumn, "spreadsheet data.endColumn");
  if (startRow >= endRow || startColumn >= endColumn) {
    throw new Error("spreadsheet data 窗口必须是半开区间 [start, end)");
  }
  const cells = asArray(record.cells, "spreadsheet data.cells").map((raw, index) => {
    const cell = asRecord(raw, `spreadsheet data.cells[${index}]`);
    const address = asRecord(cell.address, `spreadsheet data.cells[${index}].address`);
    const row = asNonNegativeInteger(address.row, `spreadsheet data.cells[${index}].address.row`);
    const column = asNonNegativeInteger(address.column, `spreadsheet data.cells[${index}].address.column`);
    if (address.sheetId !== sheetId) throw new Error(`spreadsheet data.cells[${index}] sheetId 不一致`);
    return {
      address: { sheetId, row, column },
      ...(cell.value === undefined || cell.value === null ? {} : { value: cell.value }),
      ...(cell.formula === undefined || cell.formula === null ? {} : { formula: asString(cell.formula, `spreadsheet data.cells[${index}].formula`) }),
      attrs: cell.attrs === undefined || cell.attrs === null ? {} : asRecord(cell.attrs, `spreadsheet data.cells[${index}].attrs`),
      ...(cell.style === undefined || cell.style === null ? {} : { style: parseCellStyle(cell.style, `spreadsheet data.cells[${index}].style`) }),
    };
  });
  if (cells.length !== record.cellCount) throw new Error("spreadsheet data.cellCount 与 cells 数量不一致");
  const sparse = asBoolean(record.sparse, "spreadsheet data.sparse");
  const valuesRecord = asRecord(record.values, "spreadsheet data.values");
  const values: Record<string, SpreadsheetCalculatedValue> = {};
  for (const [key, raw] of Object.entries(valuesRecord)) {
    values[key] = parseSpreadsheetCalculatedValue(raw, `spreadsheet data.values["${key}"]`);
  }
  const conditionalStyles: Record<string, string[]> = {};
  if (record.conditionalStyles !== undefined && record.conditionalStyles !== null) {
    const stylesRecord = asRecord(record.conditionalStyles, "spreadsheet data.conditionalStyles");
    for (const [key, raw] of Object.entries(stylesRecord)) {
      conditionalStyles[key] = asArray(raw, `spreadsheet data.conditionalStyles["${key}"]`).map(
        (entry, index) => asString(entry, `spreadsheet data.conditionalStyles["${key}"][${index}]`),
      );
    }
  }
  const filteredOutRows = record.filteredOutRows === undefined || record.filteredOutRows === null
    ? []
    : asArray(record.filteredOutRows, "spreadsheet data.filteredOutRows").map(
        (entry, index) => asNonNegativeInteger(entry, `spreadsheet data.filteredOutRows[${index}]`),
      );
  return { sheetId, startRow, endRow, startColumn, endColumn, cells, cellCount: record.cellCount, sparse, values, conditionalStyles, filteredOutRows };
}

function parseSpreadsheetCalculatedValue(value: unknown, name: string): SpreadsheetCalculatedValue {
  const record = asRecord(value, name);
  switch (record.type) {
    case "blank":
      return { type: "blank" };
    case "number":
      return { type: "number", value: asFiniteNumber(record.value, `${name}.value`) };
    case "text":
      return { type: "text", value: asString(record.value, `${name}.value`) };
    case "bool":
      return { type: "bool", value: asBoolean(record.value, `${name}.value`) };
    case "error": {
      const detail = asRecord(record.value, `${name}.value`);
      return { type: "error", value: { code: asString(detail.code, `${name}.value.code`), message: asString(detail.message, `${name}.value.message`) } };
    }
    default:
      throw new Error(`${name} 类型无效：${String(record.type)}`);
  }
}

export function parsePresentationDeckProjection(value: unknown): PresentationDeckProjection {
  const record = asRecord(value, "presentation.data");
  const pageSpec = asRecord(record.pageSpec, "presentation.data.pageSpec");
  const width = asFinitePositiveNumber(pageSpec.width, "presentation.data.pageSpec.width");
  const height = asFinitePositiveNumber(pageSpec.height, "presentation.data.pageSpec.height");
  const unit = pageSpec.unit;
  if (unit !== "emu" && unit !== "point") throw new Error("presentation.data.pageSpec.unit 无效");
  const masters = asArray(record.masters, "presentation.data.masters").map((value, index) => {
    const master = asRecord(value, `presentation.data.masters[${index}]`);
    const entity = parsePresentationV5Master(master.master);
    const id = asNonEmptyString(master.id, `presentation.data.masters[${index}].id`);
    if (entity.id !== id) throw new Error(`presentation.data.masters[${index}].master.id 不一致`);
    return {
      id,
      name: asString(master.name, `presentation.data.masters[${index}].name`),
      placeholderCount: asNonNegativeInteger(master.placeholderCount, `presentation.data.masters[${index}].placeholderCount`),
      master: entity,
    };
  });
  const masterIds = new Set(masters.map((master) => master.id));
  const layouts = asArray(record.layouts, "presentation.data.layouts").map((value, index) => {
    const layout = asRecord(value, `presentation.data.layouts[${index}]`);
    const masterId = asNonEmptyString(layout.masterId, `presentation.data.layouts[${index}].masterId`);
    if (!masterIds.has(masterId)) throw new Error(`presentation.data.layouts[${index}] 引用不存在 master`);
    const entity = parsePresentationV5Layout(layout.layout, masters.map((master) => master.master));
    const id = asNonEmptyString(layout.id, `presentation.data.layouts[${index}].id`);
    if (entity.id !== id || entity.masterId !== masterId) throw new Error(`presentation.data.layouts[${index}].layout 标识不一致`);
    return {
      id,
      masterId,
      name: asString(layout.name, `presentation.data.layouts[${index}].name`),
      placeholderCount: asNonNegativeInteger(layout.placeholderCount, `presentation.data.layouts[${index}].placeholderCount`),
      layout: entity,
    };
  });
  if (masters.length !== asNonNegativeInteger(record.masterCount, "presentation.data.masterCount")) {
    throw new Error("presentation.data.masterCount 与 masters 不一致");
  }
  if (layouts.length !== asNonNegativeInteger(record.layoutCount, "presentation.data.layoutCount")) {
    throw new Error("presentation.data.layoutCount 与 layouts 不一致");
  }
  return {
    pageSpec: { width, height, unit, ...(pageSpec.safeArea === undefined ? {} : { safeArea: pageSpec.safeArea }) },
    themeId: asNonEmptyString(record.themeId, "presentation.data.themeId"),
    themeName: asString(record.themeName, "presentation.data.themeName"),
    slideCount: asNonNegativeInteger(record.slideCount, "presentation.data.slideCount"),
    masterCount: masters.length,
    layoutCount: layouts.length,
    assetCount: asNonNegativeInteger(record.assetCount, "presentation.data.assetCount"),
    masters,
    layouts,
  };
}

export function parsePresentationOutline(value: unknown): { items: PresentationSlideOutlineItem[] } {
  const record = asRecord(value, "presentation outline.data");
  return {
    items: asArray(record.items, "presentation outline.data.items").map((item, index) => {
      const slide = asRecord(item, `presentation outline.data.items[${index}]`);
      return {
        slideId: asNonEmptyString(slide.slideId, "presentation slide.slideId"),
        orderKey: asNonEmptyString(slide.orderKey, "presentation slide.orderKey"),
        name: asString(slide.name, "presentation slide.name"),
        layoutId: nullableString(slide.layoutId, "presentation slide.layoutId"),
        nodeCount: asNonNegativeInteger(slide.nodeCount, "presentation slide.nodeCount"),
        timelineEntryCount: asNonNegativeInteger(slide.timelineEntryCount, "presentation slide.timelineEntryCount"),
        hasNotes: asBoolean(slide.hasNotes, "presentation slide.hasNotes"),
      };
    }),
  };
}

export function parsePresentationSlideProjection(value: unknown): PresentationSlideProjection {
  const record = asRecord(value, "presentation slide.data");
  const assetIds = record.assetIds === undefined ? undefined : asNonEmptyStringArray(record.assetIds, "presentation slide.assetIds");
  const rawNodes = record.nodes === undefined ? undefined : asArray(record.nodes, "presentation slide.nodes");
  if (rawNodes && !assetIds) throw new Error("presentation slide.nodes 必须同时提供 assetIds");
  const slideNodeIds = rawNodes?.map((node, index) => {
    const candidate = asRecord(node, `presentation slide.nodes[${index}]`);
    return asNonEmptyString(candidate.id, `presentation slide.nodes[${index}].id`);
  }) ?? [];
  const result: PresentationSlideProjection = {
    slideId: asNonEmptyString(record.slideId, "presentation slide.slideId"),
    orderKey: asNonEmptyString(record.orderKey, "presentation slide.orderKey"),
    name: asString(record.name, "presentation slide.name"),
    layoutId: nullableString(record.layoutId, "presentation slide.layoutId"),
    background: record.background,
  };
  if (record.transition !== undefined) result.transition = record.transition === null ? null : parsePresentationTransition(record.transition, "presentation slide.transition");
  if (record.notes !== undefined) result.notes = nullableString(record.notes, "presentation slide.notes");
  if (rawNodes && assetIds) {
    result.assetIds = assetIds;
    result.nodes = rawNodes.map((node) => parsePresentationV5ProjectedNode(node, { assetIds, slideNodeIds }));
  }
  if (record.timeline !== undefined) result.timeline = parsePresentationTimeline(record.timeline, "presentation slide.timeline", slideNodeIds);
  return result;
}

function parsePresentationTransition(value: unknown, name: string): PresentationV5SlideTransition {
  const record = asRecord(value, name);
  const kind = record.kind;
  if (kind !== "none" && kind !== "fade" && kind !== "push" && kind !== "wipe") throw new Error(`${name}.kind 无效`);
  return { kind, durationMs: asNonNegativeInteger(record.durationMs, `${name}.durationMs`) };
}

export function parsePresentationNodeProjection(value: unknown): PresentationNodeProjection {
  const record = asRecord(value, "presentation node.data");
  const assetIds = asNonEmptyStringArray(record.assetIds, "presentation node.assetIds");
  const slideNodeIds = asNonEmptyStringArray(record.slideNodeIds, "presentation node.slideNodeIds");
  const node = parsePresentationV5ProjectedNode(record.node, { assetIds, slideNodeIds });
  if (!slideNodeIds.includes(node.id)) throw new Error("presentation node.node 必须属于 slideNodeIds");
  const childNodeIds = asNonEmptyStringArray(record.childNodeIds, "presentation node.childNodeIds");
  if (childNodeIds.some((id) => !slideNodeIds.includes(id))) {
    throw new Error("presentation node.childNodeIds 引用不存在 slide node");
  }
  return {
    slideId: asNonEmptyString(record.slideId, "presentation node.slideId"),
    node,
    childNodeIds,
    assetIds,
    slideNodeIds,
  };
}

function parsePresentationTimeline(value: unknown, name: string, nodeIds: string[]): PresentationV5Slide["timeline"] {
  const timeline = asRecord(value, name);
  const entries = asArray(timeline.entries, `${name}.entries`).map((entry, index) => {
    const item = asRecord(entry, `${name}.entries[${index}]`);
    const targetNodeId = asNonEmptyString(item.targetNodeId, `${name}.entries[${index}].targetNodeId`);
    if (!nodeIds.includes(targetNodeId)) throw new Error(`${name}.entries[${index}] 引用不存在 node`);
    const trigger = item.trigger;
    if (trigger !== "onClick" && trigger !== "withPrevious" && trigger !== "afterPrevious") throw new Error(`${name}.entries[${index}].trigger 无效`);
    const preset = item.preset;
    if (preset !== "appear" && preset !== "fade" && preset !== "flyIn" && preset !== "wipe") throw new Error(`${name}.entries[${index}].preset 无效`);
    return {
      id: asNonEmptyString(item.id, `${name}.entries[${index}].id`),
      targetNodeId,
      trigger: trigger as PresentationV5TimelineEntry["trigger"],
      preset: preset as PresentationV5TimelineEntry["preset"],
      durationMs: asNonNegativeInteger(item.durationMs, `${name}.entries[${index}].durationMs`),
      delayMs: asNonNegativeInteger(item.delayMs, `${name}.entries[${index}].delayMs`),
      orderKey: asNonEmptyString(item.orderKey, `${name}.entries[${index}].orderKey`),
    };
  });
  return { entries };
}

/** Parse the stable list projection shared by outline and block-reference routes. */
export function parseProjectionItems(value: unknown, name = "projection.data"): { items: ProjectionItem[] } {
  const record = asRecord(value, name);
  return {
    items: asArray(record.items, `${name}.items`).map((item, index) => parseProjectionItem(item, `${name}.items[${index}]`)),
  };
}

/** Parse the block route with content/payload/citation fields preserved. */
export function parseBlockProjectionItems(value: unknown, name = "projection.data"): { items: BlockProjectionItem[] } {
  const record = asRecord(value, name);
  return {
    items: asArray(record.items, `${name}.items`).map((item, index) =>
      parseProjectionItem(item, `${name}.items[${index}]`),
    ),
  };
}

/** Parse one block projection without exposing a writable DocumentModel. */
export function parseProjectionItem(value: unknown, name = "projection.item"): BlockProjectionItem {
  const record = asRecord(value, name);
  const children = asArray(record.children, `${name}.children`).map((child, index) => {
    const childRecord = asRecord(child, `${name}.children[${index}]`);
    return {
      blockId: asNonEmptyString(childRecord.blockId, `${name}.children[${index}].blockId`),
      kind: asNonEmptyString(childRecord.kind, `${name}.children[${index}].kind`),
    };
  });
  const item: BlockProjectionItem = {
    blockId: asNonEmptyString(record.blockId, `${name}.blockId`),
    kind: asNonEmptyString(record.kind, `${name}.kind`),
    parentId: record.parentId === null ? null : asNonEmptyString(record.parentId, `${name}.parentId`),
    order: asNonNegativeInteger(record.order, `${name}.order`),
    children,
  };
  if (record.content !== undefined) item.content = record.content;
  if (record.payload !== undefined) item.payload = record.payload;
  if (record.headingPath !== undefined) item.headingPath = asStringArray(record.headingPath, `${name}.headingPath`);
  if (record.sourceRef !== undefined) item.sourceRef = parseSourceRef(record.sourceRef, `${name}.sourceRef`);
  if (record.refs !== undefined) {
    item.refs = asArray(record.refs, `${name}.refs`).map((ref, index) => parseCitationRef(ref, `${name}.refs[${index}]`));
  }
  return item;
}

function parseSourceRef(value: unknown, name: string): { artifactId: string; blockId: string } {
  const record = asRecord(value, name);
  return {
    artifactId: asNonEmptyString(record.artifactId, `${name}.artifactId`),
    blockId: asNonEmptyString(record.blockId, `${name}.blockId`),
  };
}

function parseCitationRef(value: unknown, name: string): CitationRef {
  const record = asRecord(value, name);
  const range = asRecord(record.textRange, `${name}.textRange`);
  const start = asNonNegativeInteger(range.start, `${name}.textRange.start`);
  const end = asNonNegativeInteger(range.end, `${name}.textRange.end`);
  if (end < start) throw new Error(`${name}.textRange.end 必须不小于 start`);
  return {
    artifactId: asNonEmptyString(record.artifactId, `${name}.artifactId`),
    blockId: asNonEmptyString(record.blockId, `${name}.blockId`),
    revision: asNonNegativeInteger(record.revision, `${name}.revision`),
    textRange: { start, end },
    headingPath: asStringArray(record.headingPath, `${name}.headingPath`),
    ...(record.sourceUrl === undefined ? {} : { sourceUrl: asString(record.sourceUrl, `${name}.sourceUrl`) }),
  };
}

export function parseEventPage(value: unknown): EventPage {
  const record = asRecord(value, "event page");
  const events = asArray(record.events, "event page.events").map((event, index) => {
    const item = asRecord(event, `event page.events[${index}]`);
    return {
      eventId: asNonEmptyString(item.eventId, "event.eventId"),
      artifactId: asNonEmptyString(item.artifactId, "event.artifactId"),
      transactionId: asNonEmptyString(item.transactionId, "event.transactionId"),
      revision: asNonNegativeInteger(item.revision, "event.revision"),
      typeId: asNonEmptyString(item.typeId, "event.typeId"),
      payload: item.payload,
    };
  });
  return {
    artifactId: asNonEmptyString(record.artifactId, "event page.artifactId"),
    revision: asNonNegativeInteger(record.revision, "event page.revision"),
    events,
    ...(record.nextCursor === undefined ? {} : { nextCursor: asString(record.nextCursor, "event page.nextCursor") }),
  };
}

export function parseArtifactTransactionResult(value: unknown): ArtifactTransactionResult {
  const record = asRecord(value, "artifact transaction result");
  return {
    ...parseCommitResult(value),
    canUndo: asBoolean(record.canUndo, "artifact transaction result.canUndo"),
    canRedo: asBoolean(record.canRedo, "artifact transaction result.canRedo"),
  };
}

export function parseArtifactTransactionHistoryState(value: unknown): ArtifactTransactionHistoryState {
  const record = asRecord(value, "artifact history state");
  return {
    canUndo: asBoolean(record.canUndo, "artifact history state.canUndo"),
    canRedo: asBoolean(record.canRedo, "artifact history state.canRedo"),
  };
}

export function parseApiError(value: unknown): ApiErrorEnvelope {
  const record = asRecord(value, "api error");
  return {
    error: asString(record.error, "api error.error"),
    code: asNonEmptyString(record.code, "api error.code"),
    requestId: asNonEmptyString(record.requestId, "api error.requestId"),
    retryable: asBoolean(record.retryable, "api error.retryable"),
    ...(record.details === undefined ? {} : { details: asRecord(record.details, "api error.details") }),
  };
}

export function parseArtifactMeta(value: unknown, name = "artifact meta"): ArtifactMeta {
  const record = asRecord(value, name);
  return {
    id: asNonEmptyString(record.id, `${name}.id`),
    kind: asArtifactKind(record.kind, `${name}.kind`),
    title: asString(record.title, `${name}.title`),
    ownerId: asNonEmptyString(record.ownerId, `${name}.ownerId`),
    size: asNonNegativeInteger(record.size, `${name}.size`),
    version: asNonNegativeInteger(record.version, `${name}.version`),
    starred: asBoolean(record.starred, `${name}.starred`),
    createdAt: asString(record.createdAt, `${name}.createdAt`),
    updatedAt: asString(record.updatedAt, `${name}.updatedAt`),
  };
}

function asRecord(value: unknown, name: string): Record<string, unknown> {
  if (typeof value !== "object" || value === null || Array.isArray(value)) throw new Error(`${name} 必须是对象`);
  return value as Record<string, unknown>;
}
function asArray(value: unknown, name: string): unknown[] {
  if (!Array.isArray(value)) throw new Error(`${name} 必须是数组`);
  return value;
}
function asStringArray(value: unknown, name: string): string[] {
  return asArray(value, name).map((item, index) => asString(item, `${name}[${index}]`));
}
function asString(value: unknown, name: string): string {
  if (typeof value !== "string") throw new Error(`${name} 必须是字符串`);
  return value;
}
function nullableString(value: unknown, name: string): string | null {
  return value === null ? null : asString(value, name);
}
function asNonEmptyString(value: unknown, name: string): string {
  const result = asString(value, name);
  if (!result.trim()) throw new Error(`${name} 不能为空`);
  return result;
}
function asNonEmptyStringArray(value: unknown, name: string): string[] {
  return asArray(value, name).map((item, index) => asNonEmptyString(item, `${name}[${index}]`));
}
function asNonNegativeInteger(value: unknown, name: string): number {
  if (typeof value !== "number" || !Number.isSafeInteger(value) || value < 0) throw new Error(`${name} 必须是非负整数`);
  return value;
}
function asFinitePositiveNumber(value: unknown, name: string): number {
  if (typeof value !== "number" || !Number.isFinite(value) || value <= 0) throw new Error(`${name} 必须是正有限数`);
  return value;
}
function asFiniteNumber(value: unknown, name: string): number {
  if (typeof value !== "number" || !Number.isFinite(value)) throw new Error(`${name} 必须是有限数`);
  return value;
}
function asPositiveInteger(value: unknown, name: string): number {
  const result = asNonNegativeInteger(value, name);
  if (result < 1) throw new Error(`${name} 必须是正整数`);
  return result;
}
function asBoolean(value: unknown, name: string): boolean {
  if (typeof value !== "boolean") throw new Error(`${name} 必须是布尔值`);
  return value;
}
function asArtifactKind(value: unknown, name: string): ArtifactKind {
  if (value === "document" || value === "spreadsheet" || value === "presentation" || value === "mindmap" || value === "whiteboard") return value;
  throw new Error(`${name} 类型无效`);
}
function asCapabilityStatus(value: unknown, name: string): CapabilityStatus {
  if (value === "stable" || value === "preview" || value === "planned" || value === "unsupported") return value;
  throw new Error(`${name} 无效`);
}
function asProjectionKind(value: unknown, name: string): ProjectionKind {
  if (
    value === "outline" || value === "tableOfContents" || value === "documentPrint" || value === "block" || value === "presentation" ||
    value === "presentationOutline" || value === "presentationSlide" ||
    value === "presentationNode" || value === "mindmap" ||
    value === "whiteboard" || value === "spreadsheet"
  ) return value;
  throw new Error(`${name} 无效`);
}

// Keep these imports in this boundary module so generated API consumers can
// use one package entry point without re-exporting engine internals.
export type { ArtifactCommandEnvelope, CommitResult, DocumentBlock, SnapshotEnvelope };
