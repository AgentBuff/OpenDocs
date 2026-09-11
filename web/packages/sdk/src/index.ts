import {
  ArtifactApiClient,
  ArtifactApiError,
  type ArtifactApiClientOptions,
  type ArtifactExportOptions,
  type ArtifactImportResult,
  type ArtifactTransactionHistoryState,
  type ArtifactTransactionResult,
  type BlockProjectionItem,
  type CitationRef,
  type EventPage,
  type EventRecord,
  type DocumentTocItem,
  type DocumentPrintProjection,
  type ProjectionEnvelope,
  type ProjectionItem,
  type ProjectionRequest,
  type PresentationDeckProjection,
  type PresentationNodeProjection,
  type PresentationNodeRequest,
  type PresentationOutlineRequest,
  type PresentationSlideProjection,
  type PresentationSlideRequest,
  type PresentationPresencePage,
  type PresentationPresenceUpdate,
  type DocumentPresencePage,
  type DocumentPresenceUpdate,
  type DocumentReviewPage,
  type DocumentReviewThread,
  type CreateDocumentReview,
  type CreateDocumentSuggestion,
  type MindmapProjection,
  type SpreadsheetGridProjection,
  type SpreadsheetProjectionRequest,
  type SpreadsheetStructureProjection,
} from "@open-office/schema/api";
import type {
  ArtifactCommandEnvelope,
  CommandRecord,
  TransactionOrigin,
} from "@open-office/schema/artifact";

/** A command input deliberately carries semantic type ids, never DOM actions. */
export interface SemanticCommandInput {
  typeId: string;
  payload: Record<string, unknown>;
  commandId?: string;
}

export interface TransactionInput {
  artifactId: string;
  baseRevision: number;
  actorId: string;
  commands: readonly SemanticCommandInput[];
  transactionId?: string;
  intentId?: string;
  /** External clients use the existing remote origin; no Agent origin is added to the protocol. */
  origin?: TransactionOrigin;
  protocolVersion?: number;
}

export interface AgentContextOptions extends ProjectionRequest {
  /** Context reads content, heading paths and citations unless explicitly overridden. */
  include?: ProjectionRequest["include"];
}

export interface AgentContext {
  artifactId: string;
  outline: ProjectionEnvelope<{ items: ProjectionItem[] }>;
  blocks: ProjectionEnvelope<{ items: BlockProjectionItem[] }>;
  citations: CitationRef[];
}

/**
 * Small, framework-free SDK facade for agent/CLI/MCP adapters.
 *
 * It is intentionally a thin composition over ArtifactApiClient. It does not
 * expose engine state, a DOM operation, an LLM runtime or a second protocol.
 */
export class OpenOfficeSdk {
  readonly api: ArtifactApiClient;

  constructor(options: ArtifactApiClientOptions = {}) {
    this.api = new ArtifactApiClient(options);
  }

  capabilities() {
    return this.api.capabilities();
  }

  listArtifacts() {
    return this.api.listArtifacts();
  }

  importArtifact(file: Blob, fileName: string, mode: "audit" | "strict" = "audit"): Promise<ArtifactImportResult> {
    return this.api.importArtifact(file, fileName, mode);
  }

  exportArtifact(artifactId: string, format: "json" | "md" | "svg" | "pdf", options: ArtifactExportOptions = {}): Promise<Blob> {
    return this.api.exportArtifact(artifactId, format, options);
  }

  /** Read bounded structure/content and citation refs without downloading a full snapshot. */
  async context(artifactId: string, options: AgentContextOptions = {}): Promise<AgentContext> {
    const include = options.include ?? ["content", "headingPath", "refs"];
    const [outline, blocks] = await Promise.all([
      this.api.outline(artifactId, {
        parentId: options.parentId,
        cursor: options.cursor,
        limit: options.limit,
        maxBytes: options.maxBytes,
        include: include.filter((item) => item !== "content" && item !== "refs") as Array<"headingPath">,
      }),
      this.api.blocks(artifactId, { ...options, include }),
    ]);
    const citations = blocks.data.items.flatMap((item) => item.refs ?? []);
    return { artifactId, outline, blocks, citations };
  }

  outline(artifactId: string, options: ProjectionRequest = {}) {
    return this.api.outline(artifactId, options);
  }

  tableOfContents(
    artifactId: string,
    options: ProjectionRequest = {},
  ): Promise<ProjectionEnvelope<{ items: DocumentTocItem[] }>> {
    return this.api.tableOfContents(artifactId, options);
  }

  documentPrint(artifactId: string): Promise<ProjectionEnvelope<DocumentPrintProjection>> {
    return this.api.documentPrint(artifactId);
  }

  blocks(artifactId: string, options: ProjectionRequest = {}) {
    return this.api.blocks(artifactId, options);
  }

  block(artifactId: string, blockId: string, options: ProjectionRequest = {}): Promise<ProjectionEnvelope<BlockProjectionItem>> {
    return this.api.block(artifactId, blockId, options);
  }

  /**
   * Strict read-only Presentation Deck facts.  Presentation writes remain
   * capability-gated: callers must first discover `presentation: stable` and
   * submit only one of the advertised semantic command type ids.
   */
  presentation(artifactId: string): Promise<ProjectionEnvelope<PresentationDeckProjection>> {
    return this.api.presentation(artifactId);
  }

  /** Derived graph geometry for a Mindmap renderer; command writes stay on submit(). */
  mindmap(artifactId: string, theme?: MindmapProjection["theme"]): Promise<ProjectionEnvelope<MindmapProjection>> {
    return this.api.mindmap(artifactId, theme);
  }

  /** Ephemeral collaborator state; this intentionally bypasses transactions. */
  presentationPresence(artifactId: string): Promise<PresentationPresencePage> {
    return this.api.presentationPresence(artifactId);
  }

  updatePresentationPresence(artifactId: string, sessionId: string, update: PresentationPresenceUpdate): Promise<void> {
    return this.api.updatePresentationPresence(artifactId, sessionId, update);
  }

  documentPresence(artifactId: string): Promise<DocumentPresencePage> {
    return this.api.documentPresence(artifactId);
  }

  updateDocumentPresence(artifactId: string, sessionId: string, update: DocumentPresenceUpdate): Promise<void> {
    return this.api.updateDocumentPresence(artifactId, sessionId, update);
  }

  documentReviews(artifactId: string): Promise<DocumentReviewPage> {
    return this.api.documentReviews(artifactId);
  }

  createDocumentReview(artifactId: string, review: CreateDocumentReview): Promise<DocumentReviewPage> {
    return this.api.createDocumentReview(artifactId, review);
  }

  createDocumentSuggestion(artifactId: string, suggestion: CreateDocumentSuggestion): Promise<DocumentReviewPage> {
    return this.api.createDocumentSuggestion(artifactId, suggestion);
  }

  updateDocumentReview(artifactId: string, threadId: string, state: DocumentReviewThread["state"]): Promise<void> {
    return this.api.updateDocumentReview(artifactId, threadId, state);
  }

  history(artifactId: string): Promise<ArtifactTransactionHistoryState> {
    return this.api.history(artifactId);
  }

  presentationOutline(
    artifactId: string,
    options: PresentationOutlineRequest = {},
  ) {
    return this.api.presentationOutline(artifactId, options);
  }

  presentationSlide(
    artifactId: string,
    slideId: string,
    options: PresentationSlideRequest = {},
  ): Promise<ProjectionEnvelope<PresentationSlideProjection>> {
    return this.api.presentationSlide(artifactId, slideId, options);
  }

  presentationNode(
    artifactId: string,
    slideId: string,
    nodeId: string,
    options: PresentationNodeRequest = {},
  ): Promise<ProjectionEnvelope<PresentationNodeProjection>> {
    return this.api.presentationNode(artifactId, slideId, nodeId, options);
  }

  /** Read a bounded, read-only spreadsheet grid window. Spreadsheet writes remain
   * capability-gated: callers must first discover `spreadsheet: stable` and submit
   * only an advertised semantic command type id. */
  spreadsheet(
    artifactId: string,
    options: SpreadsheetProjectionRequest,
  ): Promise<ProjectionEnvelope<SpreadsheetGridProjection>> {
    return this.api.spreadsheet(artifactId, options);
  }

  spreadsheetStructure(
    artifactId: string,
  ): Promise<ProjectionEnvelope<SpreadsheetStructureProjection>> {
    return this.api.spreadsheetStructure(artifactId);
  }

  /**
   * Immutable Artifact asset resource. This is a read URL only; edits still
   * require an advertised semantic transaction command.
   */
  assetUrl(artifactId: string, assetId: string): string {
    return this.api.assetUrl(artifactId, assetId);
  }

  events(artifactId: string, options: { sinceRevision?: number; cursor?: string; limit?: number } = {}): Promise<EventPage> {
    return this.api.events(artifactId, options);
  }

  buildTransaction(input: TransactionInput): ArtifactCommandEnvelope {
    return buildTransaction(input);
  }

  submit(input: TransactionInput | ArtifactCommandEnvelope): Promise<ArtifactTransactionResult> {
    return this.api.submitTransaction(isEnvelope(input) ? input : buildTransaction(input));
  }

  /** Create a cursor-aware, at-least-once event consumer for one artifact. */
  eventFeed(artifactId: string, options: EventFeedOptions = {}): EventFeedConsumer {
    return new EventFeedConsumer(this.api, artifactId, options);
  }
}

export interface EventFeedOptions {
  /** Usually the revision of the snapshot used to seed the external index. */
  revision?: number;
  cursor?: string;
  seenEventIds?: Iterable<string>;
}

export interface RevisionGap {
  expectedRevision: number;
  actualRevision: number;
}

export interface EventBatch {
  events: EventRecord[];
  nextCursor?: string;
  latestPageRevision: number;
  revisionGap: RevisionGap | null;
}

/**
 * Event outbox consumption helper. The server is at-least-once: eventId is
 * the dedupe key and cursor is opaque. A revision gap is surfaced to the
 * caller so it can re-read a projection/snapshot instead of guessing state.
 */
export class EventFeedConsumer {
  private readonly seen = new Set<string>();
  private cursor: string | undefined;
  private lastRevision: number;

  constructor(
    private readonly api: ArtifactApiClient,
    private readonly artifactId: string,
    options: EventFeedOptions = {},
  ) {
    this.lastRevision = options.revision ?? 0;
    this.cursor = options.cursor;
    for (const eventId of options.seenEventIds ?? []) this.seen.add(eventId);
  }

  get revision(): number {
    return this.lastRevision;
  }

  get nextCursor(): string | undefined {
    return this.cursor;
  }

  async next(limit?: number): Promise<EventBatch> {
    const page = await this.api.events(this.artifactId, this.cursor ? { cursor: this.cursor, limit } : {
      sinceRevision: this.lastRevision,
      limit,
    });
    return this.accept(page);
  }

  accept(page: EventPage): EventBatch {
    if (page.artifactId !== this.artifactId) {
      throw new Error(`event page artifactId 不匹配：期望 ${this.artifactId}，实际 ${page.artifactId}`);
    }
    let revisionGap: RevisionGap | null = null;
    const events: EventRecord[] = [];
    for (const event of page.events) {
      if (this.seen.has(event.eventId)) continue;
      if (event.revision > this.lastRevision + 1 && revisionGap === null) {
        revisionGap = { expectedRevision: this.lastRevision + 1, actualRevision: event.revision };
      }
      this.seen.add(event.eventId);
      events.push(event);
      this.lastRevision = Math.max(this.lastRevision, event.revision);
    }
    this.cursor = page.nextCursor;
    return {
      events,
      ...(page.nextCursor === undefined ? {} : { nextCursor: page.nextCursor }),
      latestPageRevision: page.revision,
      revisionGap,
    };
  }
}

export function buildTransaction(input: TransactionInput): ArtifactCommandEnvelope {
  assertNonEmpty(input.artifactId, "artifactId");
  assertNonEmpty(input.actorId, "actorId");
  assertRevision(input.baseRevision, "baseRevision");
  if (!Array.isArray(input.commands) || input.commands.length === 0) {
    throw new Error("commands 必须至少包含一个 semantic command");
  }
  const commands: CommandRecord[] = input.commands.map((command, index) => {
    assertNonEmpty(command.typeId, `commands[${index}].typeId`);
    if (typeof command.payload !== "object" || command.payload === null || Array.isArray(command.payload)) {
      throw new Error(`commands[${index}].payload 必须是对象`);
    }
    return {
      commandId: command.commandId ?? randomId(),
      typeId: command.typeId,
      payload: command.payload,
    };
  });
  return {
    protocolVersion: input.protocolVersion ?? 1,
    transactionId: input.transactionId ?? randomId(),
    intentId: input.intentId ?? randomId(),
    artifactId: input.artifactId,
    actorId: input.actorId,
    baseRevision: input.baseRevision,
    origin: input.origin ?? "remote",
    commands,
  };
}

export function isVersionConflict(error: unknown): error is ArtifactApiError {
  return error instanceof ArtifactApiError && error.status === 409 && error.envelope?.code === "version_conflict";
}

export function isRetryableApiError(error: unknown): error is ArtifactApiError {
  return error instanceof ArtifactApiError && error.envelope?.retryable === true;
}

export function conflictDetails(error: unknown): Record<string, unknown> | null {
  return isVersionConflict(error) ? error.envelope?.details ?? null : null;
}

function isEnvelope(value: TransactionInput | ArtifactCommandEnvelope): value is ArtifactCommandEnvelope {
  return "protocolVersion" in value && "transactionId" in value && "intentId" in value;
}

function assertNonEmpty(value: string, name: string): void {
  if (typeof value !== "string" || !value.trim()) throw new Error(`${name} 不能为空`);
}

function assertRevision(value: number, name: string): void {
  if (!Number.isSafeInteger(value) || value < 0) throw new Error(`${name} 必须是非负整数`);
}

function randomId(): string {
  return globalThis.crypto?.randomUUID?.() ?? `${Date.now()}-${Math.random().toString(16).slice(2)}`;
}
