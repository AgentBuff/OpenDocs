import {
  parseCommitResult,
  type ArtifactCommandEnvelope,
  type CommitResult,
  type DocumentBlock,
  type SnapshotEnvelope,
  type ArtifactKind,
} from "./artifact.js";
import {
  parsePresentationV5ProjectedNode,
  type PresentationV5Node,
  type PresentationV5Slide,
  type PresentationV5SlideTransition,
  type PresentationV5TimelineEntry,
} from "./presentation-v5.js";

export type CapabilityStatus = "stable" | "planned";
export type ProjectionKind = "outline" | "block" | "presentation" | "presentationOutline" | "presentationSlide" | "presentationNode";

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
  status: CapabilityStatus;
  commands: ArtifactCommandCapability[];
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

/** Transport response of an accepted transaction, including history availability. */
export interface ArtifactTransactionResult extends CommitResult, ArtifactTransactionHistoryState {}

export interface ApiErrorEnvelope {
  error: string;
  code: string;
  requestId: string;
  retryable?: boolean;
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

  async outline(
    artifactId: string,
    options: ProjectionRequest = {},
  ): Promise<ProjectionEnvelope<{ items: ProjectionItem[] }>> {
    const envelope = parseProjectionEnvelope(await this.getProjection(artifactId, "outline", options), "outline");
    return { ...envelope, data: parseProjectionItems(envelope.data, "outline.data") };
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

  /** Read compact undo/redo availability without exposing an editable log. */
  async history(artifactId: string): Promise<ArtifactTransactionHistoryState> {
    return parseArtifactTransactionHistoryState(
      await this.get(`/api/artifacts/${encodeURIComponent(artifactId)}/history`),
    );
  }

  /**
   * Read live Presentation collaborators. Presence is intentionally separate
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
    return this.request(path, { method: "GET" });
  }

  private async request(path: string, init: RequestInit): Promise<unknown> {
    const response = await this.fetcher(`${this.baseUrl}${path}`, init);
    if (response.ok) {
      if (response.status === 204) return undefined;
      return response.json() as Promise<unknown>;
    }
    let envelope: ApiErrorEnvelope | null = null;
    try {
      envelope = parseApiError(await response.json());
    } catch {
      // Keep a transport error even if a proxy returned non-JSON content.
    }
    throw new ArtifactApiError(response.status, envelope?.error ?? `HTTP ${response.status}`, envelope);
  }
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
    return {
      kind: asArtifactKind(artifact.kind, "capability kind"),
      namespace: asNonEmptyString(artifact.namespace, "capability namespace"),
      status: asCapabilityStatus(artifact.status, "capability status"),
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

export function parsePresentationDeckProjection(value: unknown): PresentationDeckProjection {
  const record = asRecord(value, "presentation.data");
  const pageSpec = asRecord(record.pageSpec, "presentation.data.pageSpec");
  const width = asFinitePositiveNumber(pageSpec.width, "presentation.data.pageSpec.width");
  const height = asFinitePositiveNumber(pageSpec.height, "presentation.data.pageSpec.height");
  const unit = pageSpec.unit;
  if (unit !== "emu" && unit !== "point") throw new Error("presentation.data.pageSpec.unit 无效");
  return {
    pageSpec: { width, height, unit, ...(pageSpec.safeArea === undefined ? {} : { safeArea: pageSpec.safeArea }) },
    themeId: asNonEmptyString(record.themeId, "presentation.data.themeId"),
    themeName: asString(record.themeName, "presentation.data.themeName"),
    slideCount: asNonNegativeInteger(record.slideCount, "presentation.data.slideCount"),
    masterCount: asNonNegativeInteger(record.masterCount, "presentation.data.masterCount"),
    layoutCount: asNonNegativeInteger(record.layoutCount, "presentation.data.layoutCount"),
    assetCount: asNonNegativeInteger(record.assetCount, "presentation.data.assetCount"),
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
    ...(record.retryable === undefined ? {} : { retryable: asBoolean(record.retryable, "api error.retryable") }),
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
  if (value === "stable" || value === "planned") return value;
  throw new Error(`${name} 无效`);
}
function asProjectionKind(value: unknown, name: string): ProjectionKind {
  if (value === "outline" || value === "block" || value === "presentation" || value === "presentationOutline" || value === "presentationSlide" || value === "presentationNode") return value;
  throw new Error(`${name} 无效`);
}

// Keep these imports in this boundary module so generated API consumers can
// use one package entry point without re-exporting engine internals.
export type { ArtifactCommandEnvelope, CommitResult, DocumentBlock, SnapshotEnvelope };
