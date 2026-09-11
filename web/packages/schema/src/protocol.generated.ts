// @generated — AUTO-GENERATED from the Rust protocol contract (ADR-0010).
// Do not edit by hand. Regenerate with:
//   node scripts/generate-protocol-typescript.mjs
// Source: crates/oo-protocol/tests/snapshots/contract_schemas.json

export interface ArtifactCapability {
/** Empty when editing is planned or unsupported. An empty list is */
/** intentional: it is not a promise that an unimplemented command exists. */
  commands: Array<ArtifactCommandCapability>;
  features: ArtifactFeatureCapabilities;
  kind: ArtifactKind;
  namespace: string;
}

export type ArtifactCapabilityStatus = "stable" | "preview" | "planned" | "unsupported";

/** A semantic command accepted by an Artifact engine. `scope` allows a client */
/** to discover table-specific commands without pretending that tables are a */
/** second Artifact model (their namespace remains `document.table`). */
export interface ArtifactCommandCapability {
  requiresRevision: boolean;
  scope: string;
  supportsIdempotency: boolean;
  typeId: string;
}

/** 一次用户意图或协同更新的网络边界。 */
/**  */
/** 这里表达的是语义 command，不是 Document engine 的内部 operation。不同 Artifact */
/** 通过 `type_id` 扩展能力，服务端必须在进入领域 engine 前做对应的 command 校验。 */
export interface ArtifactCommandEnvelope {
/** Client-local device/session identity for correlation only. Servers must */
/** derive authorization, audit author and event actor from authentication. */
  actorId: string;
  artifactId: string;
  baseRevision: number;
  commands: Array<CommandRecord>;
  intentId: string;
  origin: TransactionOrigin;
  protocolVersion: number;
  transactionId: string;
}

/** Independent product maturity signals. A client must inspect the feature it */
/** intends to use instead of treating one implemented command as proof that */
/** import, history, assets or collaboration are equally mature. */
export interface ArtifactFeatureCapabilities {
  assets: ArtifactCapabilityStatus;
  edit: ArtifactCapabilityStatus;
  export: ArtifactCapabilityStatus;
  history: ArtifactCapabilityStatus;
  import: ArtifactCapabilityStatus;
  presence: ArtifactCapabilityStatus;
  projection: ArtifactCapabilityStatus;
}

export type ArtifactKind = "document" | "spreadsheet" | "presentation" | "mindmap" | "whiteboard";

/** The write/read boundary that every Artifact client can use after discovering */
/** the capability catalog. Paths are URI templates rather than a second RPC */
/** surface, so agents, SDKs and MCP adapters all speak the same REST contract. */
export interface ArtifactTransportCapability {
  idempotencyHeader: string;
  revisionHeader: string;
  snapshotEndpoint: string;
  transactionEndpoint: string;
}

/** Versioned discovery response for agent/SDK/MCP clients. */
export interface CapabilityCatalog {
  artifacts: Array<ArtifactCapability>;
  contractVersion: number;
  protocolVersion: number;
  transport: ArtifactTransportCapability;
}

/** Command 是用户意图；payload 由对应 Artifact capability 负责校验。 */
export interface CommandRecord {
  commandId: string;
  payload: unknown;
  typeId: string;
}

/** 所有 Artifact 写入的唯一提交返回值。 */
export interface CommitResult {
  artifactId: string;
  events: Array<DomainEventRecord>;
  invalidation: Invalidation;
  mutations: Array<MutationRecord>;
  protocolVersion: number;
  revision: number;
  transactionId: string;
}

/** 提交后的领域事实。事件只能在事务成功提交后产生。 */
export interface DomainEventRecord {
  eventId: string;
  payload: unknown;
  typeId: string;
}

export interface EntityRef {
  entityId: string;
  entityType: string;
}

/** 只表达增量失效范围，不携带完整 snapshot。 */
export interface Invalidation {
  changedContainers: Array<EntityRef>;
  changedEntities: Array<EntityRef>;
  structureChanged: boolean;
}

/** 跨 Artifact 的最小持久化变更记录。具体 payload 由 Mutation registry 解释。 */
export interface MutationRecord {
  payload: unknown;
  typeId: string;
}

/** Operation 表示不进入 Artifact snapshot 的视图/协同状态，例如选区和滚动位置。 */
/** 它与 CommandRecord 有意使用不同的 ID 字段，避免把临时状态误当成持久化意图。 */
export interface OperationRecord {
  operationId: string;
  payload: unknown;
  typeId: string;
}

export type TransactionOrigin = "local" | "remote" | "undo" | "redo" | "import" | "system";
