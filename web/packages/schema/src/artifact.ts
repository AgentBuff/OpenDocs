/**
 * 版本化 Artifact 协议的浏览器边界。
 *
 * Rust `oo-schema` 是持久化格式的权威实现；这里不复制编辑逻辑，只提供 TS 的静态
 * 消费类型和运行时入口校验。所有来自网络的快照都应先经过 `parseSnapshot`，再交给
 * editor/renderer，避免把不可信 JSON 直接当成已校验模型使用。
 */

import { parsePresentationV5Deck, type PresentationV5Deck } from "./presentation-v5.js";

export type ArtifactKind =
  | "document"
  | "spreadsheet"
  | "presentation"
  | "mindmap"
  | "whiteboard";

/** Runtime accepts this version only; older snapshots must go through the offline migrator. */
export const CURRENT_SCHEMA_VERSION = 5;

export interface ArtifactPageSetup {
  width: number;
  height: number;
  marginTop: number;
  marginRight: number;
  marginBottom: number;
  marginLeft: number;
}

export interface InlineRun {
  start: number;
  end: number;
  style: InlineStyle;
}

export type Color = string;
export type VerticalAlign = "baseline" | "superscript" | "subscript";

/** Strict renderer-independent inline presentation. Unknown fields are rejected at the API boundary. */
export interface InlineStyle {
  bold: boolean;
  italic: boolean;
  underline: boolean;
  strikethrough: boolean;
  fontFamily: string | null;
  fontSize: number | null;
  color: Color | null;
  highlight: Color | null;
  verticalAlign: VerticalAlign | null;
}

export interface RichText {
  text: string;
  runs: InlineRun[];
}

/** A half-open Unicode scalar range inside one block's RichText. */
export interface TextRange {
  start: number;
  end: number;
}

/** Tri-state inline style patch: omitted keeps a field, null clears it. */
export interface InlineStylePatch {
  bold?: boolean | null;
  italic?: boolean | null;
  underline?: boolean | null;
  strikethrough?: boolean | null;
  fontFamily?: string | null;
  fontSize?: number | null;
  color?: string | null;
  highlight?: string | null;
  verticalAlign?: VerticalAlign | null;
}

export type KnownDocumentBlockKind =
  | { type: "paragraph" }
  | { type: "heading"; level: number }
  | { type: "quote" }
  | { type: "code" }
  | { type: "image" }
  | { type: "table" }
  | { type: "callout" }
  | { type: "todo" }
  | { type: "divider" }
  | { type: "page" }
  | { type: "columns" }
  | { type: "column" }
  | { type: "link" }
  | { type: "extension"; typeId: string };

/** Future block kinds keep their original object instead of being silently flattened. */
export interface UnknownDocumentBlockKind {
  type: string;
  [key: string]: unknown;
}

export type DocumentBlockKind = KnownDocumentBlockKind | UnknownDocumentBlockKind;

export interface ImageBlock {
  assetId: string;
  alt: string;
  originalAssetId: string | null;
  transform: ImageTransform;
  caption: string;
}

export interface ImageTransform {
  crop: ImageCrop;
  flipHorizontal: boolean;
  flipVertical: boolean;
}

export interface ImageCrop {
  top: number;
  right: number;
  bottom: number;
  left: number;
}

export const DEFAULT_IMAGE_TRANSFORM: Readonly<ImageTransform> = Object.freeze({
  crop: Object.freeze({ top: 0, right: 0, bottom: 0, left: 0 }),
  flipHorizontal: false,
  flipVertical: false,
});

export function defaultImageTransform(): ImageTransform {
  return { crop: { ...DEFAULT_IMAGE_TRANSFORM.crop }, flipHorizontal: false, flipVertical: false };
}

/** Todo completion is persisted block data, never renderer state or a free attribute. */
export interface TodoBlock {
  checked: boolean;
}

export interface TableColumn {
  id: string;
  width: number | null;
}

export interface TableCell {
  id: string;
  content: RichText;
  style?: TableCellStyle;
}

export interface TableCellStyle {
  fillColor?: string;
  horizontalAlign?: "left" | "center" | "right";
  verticalAlign?: "top" | "middle" | "bottom";
  borders?: TableBorderEdges;
}

export type TableBorderStyle = "solid" | "dashed" | "dotted" | "double";

export interface TableBorder {
  style: TableBorderStyle;
  color: string;
  width: number;
}

export interface TableBorderEdges {
  top?: TableBorder;
  right?: TableBorder;
  bottom?: TableBorder;
  left?: TableBorder;
  diagonalDown?: TableBorder;
  diagonalUp?: TableBorder;
}

/** Edge-level command patch: omitted leaves an edge untouched, null clears it. */
export interface TableBorderPatch {
  top?: TableBorder | null;
  right?: TableBorder | null;
  bottom?: TableBorder | null;
  left?: TableBorder | null;
  diagonalDown?: TableBorder | null;
  diagonalUp?: TableBorder | null;
}

/**
 * A grid-aware border operation. Unlike an edge patch, a preset understands
 * the selection boundary, so "outer" and "inside" never spill to cells
 * outside the selected stable-id range.
 */
export type TableBorderPreset =
  | "top"
  | "right"
  | "bottom"
  | "left"
  | "none"
  | "all"
  | "outer"
  | "inner"
  | "innerHorizontal"
  | "innerVertical"
  | "diagonalDown"
  | "diagonalUp";

export interface TableRow {
  id: string;
  height: number | null;
  cells: TableCell[];
}

export interface TableBlock {
  columns: TableColumn[];
  rows: TableRow[];
  mergedRanges: TableRange[];
}

export interface TableRange {
  startRowId: string;
  endRowId: string;
  startColumnId: string;
  endColumnId: string;
}

/** Code language/theme ids are registry keys, not a closed enum. Unknown keys are preserved so
 * adding a grammar or theme does not require a schema version bump. */
export type BuiltInCodeLanguage =
  | "plainText"
  | "javascript"
  | "typescript"
  | "rust"
  | "python"
  | "java"
  | "json"
  | "html"
  | "css"
  | "sql"
  | "bash"
  | "markdown";
export type CodeLanguage = BuiltInCodeLanguage | (string & {});
export type CodeTheme = string & {};
export type CodeIndentMode = "spaces" | "tabs";

export interface CodeBlockConfig {
  title: string;
  language: CodeLanguage;
  theme: CodeTheme;
  /** Editable viewport height in CSS pixels; the code block body scrolls beyond this height. */
  height: number;
  showLineNumbers: boolean;
  wrap: boolean;
  indentMode: CodeIndentMode;
  indentWidth: 2 | 4 | 8;
  fontSize: number;
}

export const DEFAULT_CODE_BLOCK_CONFIG: Readonly<CodeBlockConfig> = Object.freeze({
  title: "",
  language: "plainText",
  theme: "light",
  height: 200,
  showLineNumbers: true,
  wrap: false,
  indentMode: "spaces",
  indentWidth: 2,
  fontSize: 14,
});

export function defaultCodeBlockConfig(): CodeBlockConfig {
  return { ...DEFAULT_CODE_BLOCK_CONFIG };
}

export interface LinkBlock {
  url: string;
}

export type BlockAlignment = "left" | "center" | "right" | "justify";
export type ListKind = "bullet" | "ordered";

export interface ListPresentation {
  kind: ListKind;
  level: number;
}

export interface ParagraphStyleRef {
  name: string;
}

export interface BlockPresentation {
  align: BlockAlignment;
  list: ListPresentation | null;
  indentStart: number;
  indentEnd: number;
  spacingBefore: number;
  spacingAfter: number;
  lineHeight: number;
  namedStyle: ParagraphStyleRef | null;
}

export interface BlockExtension {
  typeId: string;
  raw: unknown;
}

export type BlockData =
  | { type: "none" }
  | { type: "image"; data: ImageBlock }
  | { type: "table"; data: TableBlock }
  | { type: "code"; data: CodeBlockConfig }
  | { type: "todo"; data: TodoBlock }
  | { type: "link"; data: LinkBlock }
  | { type: "extension"; data: BlockExtension };

export interface DocumentBlock {
  id: string;
  kind: DocumentBlockKind;
  presentation: BlockPresentation;
  content: RichText | null;
  children: string[];
  data: BlockData;
}

export interface DocumentModel {
  root: string[];
  blocks: DocumentBlock[];
  pageSetup: ArtifactPageSetup | null;
}

export interface SpreadsheetModel {
  metadata: SpreadsheetMetadata;
  sheets: SheetModel[];
}

export interface SheetModel {
  id: string;
  name: string;
  cells: CellModel[];
  metadata: SheetMetadata;
}

export interface CellModel {
  row: number;
  column: number;
  value?: unknown;
  formula?: string;
  attrs: Record<string, unknown>;
  style?: CellStyle | null;
}

export type CalculationMode = "automatic" | "manual";
export type DateSystem = "excel1900" | "excel1904";
export type SheetVisibility = "visible" | "hidden" | "veryHidden";

export interface SpreadsheetMetadata {
  activeSheetId: string | null;
  calculationMode: CalculationMode;
  dateSystem: DateSystem;
}

export interface FreezePane {
  rows: number;
  columns: number;
}

export interface GridRange {
  startRow: number;
  startColumn: number;
  endRow: number;
  endColumn: number;
}

export type FilterPredicate =
  | { type: "values"; value: unknown[] }
  | { type: "contains"; value: string }
  | { type: "equals"; value: unknown }
  | { type: "greaterThan"; value: number }
  | { type: "lessThan"; value: number };

export interface FilterColumn {
  column: number;
  predicate: FilterPredicate;
}

export interface FilterSpec {
  range: GridRange;
  columns: FilterColumn[];
}

export type SortDirection = "ascending" | "descending";
export interface SortKey {
  column: number;
  direction: SortDirection;
}
export interface SortSpec {
  range: GridRange;
  keys: SortKey[];
}

export type ConditionalPredicate =
  | { type: "cellIs"; value: { operator: ComparisonOperator; value: unknown } }
  | { type: "formula"; value: string }
  | { type: "colorScale"; value: { min: string; max: string } };

export type ComparisonOperator =
  | "equal"
  | "notEqual"
  | "greaterThan"
  | "greaterThanOrEqual"
  | "lessThan"
  | "lessThanOrEqual";

export interface ConditionalFormatRule {
  id: string;
  range: GridRange;
  predicate: ConditionalPredicate;
  style: CellStyle;
}

export type DataValidationKind =
  | { type: "list"; value: string[] }
  | { type: "wholeNumber"; value: { min: number; max: number } }
  | { type: "decimal"; value: { min: number; max: number } }
  | { type: "date"; value: { minSerial: number; maxSerial: number } }
  | { type: "customFormula"; value: string };

export interface DataValidationRule {
  id: string;
  range: GridRange;
  kind: DataValidationKind;
  allowBlank: boolean;
  errorMessage: string | null;
}

export interface CellStyle {
  numberFormat: string | null;
  font: FontStyle | null;
  fill: FillStyle | null;
  alignment: AlignmentStyle | null;
}
export interface FontStyle {
  family: string | null;
  size: number | null;
  bold: boolean;
  italic: boolean;
  color: string | null;
}
export interface FillStyle {
  foreground: string | null;
  background: string | null;
}
export interface AlignmentStyle {
  horizontal: string | null;
  vertical: string | null;
  wrap: boolean;
}
export interface SheetMedia {
  id: string;
  relationship: string;
  contentType: string;
  target: string;
  anchor: GridRange;
}

export interface SheetMetadata {
  visibility: SheetVisibility;
  rowCount: number | null;
  columnCount: number | null;
  freeze: FreezePane;
  autoFilter: FilterSpec | null;
  sort: SortSpec | null;
  conditionalFormats: ConditionalFormatRule[];
  dataValidations: DataValidationRule[];
  mergedRanges: GridRange[];
  media: SheetMedia[];
}

/** The only online Presentation payload. v4 generic scene graphs are offline-migration input. */
export type PresentationDeck = PresentationV5Deck;

export interface MindmapModel {
  root: string | null;
  nodes: MindmapNode[];
  edges: MindmapEdge[];
}

export interface MindmapNode {
  id: string;
  parentId: string | null;
  content: RichText | null;
  attrs: Record<string, unknown>;
  collapsed: boolean;
}

export interface MindmapEdge {
  id: string;
  sourceId: string;
  targetId: string;
  attrs: Record<string, unknown>;
}

export interface WhiteboardModel {
  elements: SceneElement[];
  camera: { x: number; y: number; scale: number };
}

export interface SceneElement {
  id: string;
  typeId: string;
  transform: { x: number; y: number; width: number; height: number; rotation: number };
  attrs: Record<string, unknown>;
  children: string[];
}

export type ArtifactPayload =
  | { kind: "document"; data: DocumentModel }
  | { kind: "spreadsheet"; data: SpreadsheetModel }
  | { kind: "presentation"; data: PresentationDeck }
  | { kind: "mindmap"; data: MindmapModel }
  | { kind: "whiteboard"; data: WhiteboardModel };

export interface ArtifactEnvelope {
  format: "open-office-artifact";
  schemaVersion: number;
  artifactId: string;
  revision: number;
  kind: ArtifactKind;
  payload: ArtifactPayload;
}

export interface SnapshotEnvelope {
  protocolVersion: number;
  artifact: ArtifactEnvelope;
}

export type TransactionOrigin = "local" | "remote" | "undo" | "redo" | "import" | "system";

/** Server-authoritative history intent. Clients never send inverse block patches. */
export type DocumentHistoryAction = "undo" | "redo";

export interface DocumentHistoryOperation {
  action: DocumentHistoryAction;
}

export const DOCUMENT_HISTORY_TYPE_ID = "document.history";

/** Command 是用户意图；payload 由对应 Artifact capability 负责校验。 */
export interface CommandRecord {
  commandId: string;
  typeId: string;
  payload: Record<string, unknown>;
}

/** Operation 只描述不进入 Artifact snapshot 的视图状态。 */
export interface OperationRecord {
  operationId: string;
  typeId: string;
  payload: Record<string, unknown>;
}

export interface CommandContext {
  artifactId: string;
  transactionId: string;
  intentId: string;
  actorId: string;
  baseRevision: number;
  origin: TransactionOrigin;
}

/** 所有 Artifact 写入的唯一网络提交 envelope。 */
export interface ArtifactCommandEnvelope extends CommandContext {
  protocolVersion: number;
  commands: CommandRecord[];
}

export interface MutationRecord {
  typeId: string;
  payload: Record<string, unknown>;
}

export interface DomainEventRecord {
  eventId: string;
  typeId: string;
  payload: Record<string, unknown>;
}

export interface EntityRef {
  entityType: string;
  entityId: string;
}

export interface Invalidation {
  changedEntities: EntityRef[];
  changedContainers: EntityRef[];
  structureChanged: boolean;
}

export interface CommitResult {
  protocolVersion: number;
  artifactId: string;
  transactionId: string;
  revision: number;
  invalidation: Invalidation;
  mutations: MutationRecord[];
  events: DomainEventRecord[];
}

export interface PendingTransaction {
  sequence: number;
  envelope: ArtifactCommandEnvelope;
}

/** Document engine 的结构事务。children 只能通过结构 command 修改。 */
export type DocumentCommand =
  | {
      type: "insertBlock";
      block: DocumentBlock;
      parentId?: string | null;
      index: number;
    }
  | {
      type: "insertQuote";
      blockId: string;
      content: RichText;
      parentId?: string | null;
      index: number;
    }
  | {
      type: "insertTodo";
      blockId: string;
      content: RichText;
      checked?: boolean;
      parentId?: string | null;
      index: number;
    }
  | {
      type: "insertLink";
      blockId: string;
      content: RichText;
      url: string;
      parentId?: string | null;
      index: number;
    }
  | {
      type: "insertDivider";
      blockId: string;
      parentId?: string | null;
      index: number;
    }
  | {
      type: "setBlockPresentation";
      blockId: string;
      patch: {
        align?: "left" | "center" | "right" | "justify" | null;
        listType?: "bullet" | "ordered" | null;
        listLevel?: number | null;
        indentLevel?: number | null;
        indentRight?: number | null;
        spacingBefore?: number | null;
        spacingAfter?: number | null;
        lineHeight?: number | null;
        namedStyle?: ParagraphStyleRef | null;
      };
    }
  | {
      type: "patchInlineRange";
      blockId: string;
      range: TextRange;
      patch: InlineStylePatch;
    }
  | {
      type: "formatTableCells";
      blockId: string;
      selection:
        | { kind: "cell"; rowId: string; cellId: string }
        | { kind: "range"; startRowId: string; endRowId: string; startColumnId: string; endColumnId: string }
        | { kind: "row"; rowId: string }
        | { kind: "column"; columnId: string }
        | { kind: "all" };
      patch: {
        textAttrs?: Record<string, unknown>;
        fillColor?: string | null;
        horizontalAlign?: "left" | "center" | "right";
        verticalAlign?: "top" | "middle" | "bottom";
      };
    }
  | {
      type: "setTableBorders";
      blockId: string;
      selection:
        | { kind: "cell"; rowId: string; cellId: string }
        | { kind: "range"; startRowId: string; endRowId: string; startColumnId: string; endColumnId: string }
        | { kind: "row"; rowId: string }
        | { kind: "column"; columnId: string }
        | { kind: "all" };
      patch: TableBorderPatch;
    }
  | {
      type: "applyTableBorderPreset";
      blockId: string;
      selection:
        | { kind: "cell"; rowId: string; cellId: string }
        | { kind: "range"; startRowId: string; endRowId: string; startColumnId: string; endColumnId: string }
        | { kind: "row"; rowId: string }
        | { kind: "column"; columnId: string }
        | { kind: "all" };
      preset: TableBorderPreset;
      border?: TableBorder;
    }
  | { type: "setTodoChecked"; blockId: string; checked: boolean }
  | { type: "convertToLink"; blockId: string; url: string }
  | { type: "setLinkTarget"; blockId: string; url: string }
  | { type: "setCodeConfig"; blockId: string; config: CodeBlockConfig }
  | {
      type: "setImageConfig";
      blockId: string;
      patch: {
        assetId?: string;
        originalAssetId?: string | null;
        transform?: ImageTransform;
        caption?: string;
      };
    }
  | { type: "replaceBlockText"; blockId: string; content: RichText }
  | { type: "convertBlock"; blockId: string; kind: DocumentBlockKind }
  | { type: "replaceTableCellText"; blockId: string; rowId: string; cellId: string; content: RichText }
  | {
      type: "patchTableCellInlineRange";
      blockId: string;
      rowId: string;
      cellId: string;
      range: TextRange;
      patch: InlineStylePatch;
    }
  | { type: "insertTableRow"; blockId: string; index: number; row: TableRow }
  | { type: "insertTableColumn"; blockId: string; index: number; column: TableColumn; cells: TableCell[] }
  | { type: "deleteTableRow"; blockId: string; rowId: string }
  | { type: "deleteTableColumn"; blockId: string; columnId: string }
  | { type: "setTableColumnWidth"; blockId: string; columnId: string; width: number }
  | { type: "setTableRowHeight"; blockId: string; rowId: string; height: number }
  | { type: "mergeTableCells"; blockId: string; range: TableRange }
  | { type: "splitTableCells"; blockId: string; range: TableRange }
  | { type: "deleteBlock"; blockId: string }
  | { type: "resetBlock"; blockId: string }
  | { type: "moveBlock"; blockId: string; parentId?: string | null; index: number }
  | { type: "setPageSetup"; pageSetup: ArtifactPageSetup | null };

export interface RemovedBlock {
  position: number;
  block: DocumentBlock;
}

/** Rust oo-document mutation journal 的稳定浏览器边界。 */
export type DocumentMutation =
  | { type: "insert"; block: DocumentBlock; parentId: string | null; index: number }
  | { type: "delete"; blockId: string; removed: RemovedBlock[]; parentId: string | null; index: number }
  | { type: "removeInserted"; block: DocumentBlock; parentId: string | null; index: number }
  | { type: "restore"; blockId: string; removed: RemovedBlock[]; parentId: string | null; index: number }
  | { type: "update"; blockId: string; before: DocumentBlock; after: DocumentBlock }
  | {
      type: "move";
      blockId: string;
      fromParentId: string | null;
      fromIndex: number;
      toParentId: string | null;
      toIndex: number;
    }
  | { type: "setPageSetup"; before: ArtifactPageSetup | null; after: ArtifactPageSetup | null };

/** 解析并校验服务端返回的 Artifact 快照。 */
export function parseSnapshot(value: unknown): SnapshotEnvelope {
  const snapshot = asRecord(value, "snapshot");
  const protocolVersion = asPositiveInteger(snapshot.protocolVersion, "protocolVersion");
  const artifact = parseArtifact(snapshot.artifact);
  if (protocolVersion > 1) {
    throw new Error(`不支持的 protocol 版本：${protocolVersion}`);
  }
  return { protocolVersion, artifact };
}

/** 解析所有 Artifact 写操作的增量提交结果。完整 snapshot 不会出现在这里。 */
export function parseCommitResult(value: unknown): CommitResult {
  const record = asRecord(value, "commitResult");
  const protocolVersion = asPositiveInteger(record.protocolVersion, "commitResult.protocolVersion");
  if (protocolVersion > 1) throw new Error(`不支持的 protocol 版本：${protocolVersion}`);
  const invalidationRecord = asRecord(record.invalidation, "commitResult.invalidation");
  const changedEntities = asArray(invalidationRecord.changedEntities, "commitResult.invalidation.changedEntities")
    .map((entity, index) => parseEntityRef(entity, `commitResult.invalidation.changedEntities[${index}]`));
  const changedContainers = asArray(invalidationRecord.changedContainers, "commitResult.invalidation.changedContainers")
    .map((entity, index) => parseEntityRef(entity, `commitResult.invalidation.changedContainers[${index}]`));
  const mutations = asArray(record.mutations, "commitResult.mutations")
    .map((mutation, index) => parseMutationRecord(mutation, `commitResult.mutations[${index}]`));
  const events = asArray(record.events, "commitResult.events")
    .map((event, index) => parseDomainEvent(event, `commitResult.events[${index}]`));
  return {
    protocolVersion,
    artifactId: asNonEmptyString(record.artifactId, "commitResult.artifactId"),
    transactionId: asNonEmptyString(record.transactionId, "commitResult.transactionId"),
    revision: asNonNegativeInteger(record.revision, "commitResult.revision"),
    invalidation: {
      changedEntities,
      changedContainers,
      structureChanged: asBoolean(invalidationRecord.structureChanged, "commitResult.invalidation.structureChanged"),
    },
    mutations,
    events,
  };
}

function parseEntityRef(value: unknown, name: string): EntityRef {
  const record = asRecord(value, name);
  return {
    entityType: asNonEmptyString(record.entityType, `${name}.entityType`),
    entityId: asNonEmptyString(record.entityId, `${name}.entityId`),
  };
}

function parseMutationRecord(value: unknown, name: string): MutationRecord {
  const record = asRecord(value, name);
  return {
    typeId: asNonEmptyString(record.typeId, `${name}.typeId`),
    payload: asRecord(record.payload, `${name}.payload`),
  };
}

function parseDomainEvent(value: unknown, name: string): DomainEventRecord {
  const record = asRecord(value, name);
  return {
    eventId: asNonEmptyString(record.eventId, `${name}.eventId`),
    typeId: asNonEmptyString(record.typeId, `${name}.typeId`),
    payload: asRecord(record.payload, `${name}.payload`),
  };
}

function parseArtifact(value: unknown): ArtifactEnvelope {
  const record = asRecord(value, "artifact");
  if (record.format !== "open-office-artifact") {
    throw new Error("Artifact format 无效");
  }
  const artifactId = asNonEmptyString(record.artifactId, "artifactId");
  const schemaVersion = asPositiveInteger(record.schemaVersion, "schemaVersion");
  const revision = asNonNegativeInteger(record.revision, "revision");
  const kind = asKind(record.kind);
  if (schemaVersion !== CURRENT_SCHEMA_VERSION) {
    throw new Error(`不支持的 schema 版本：${schemaVersion}`);
  }
  // Version gates precede payload decoding: an offline migration is the sole route for old
  // snapshots, so a v4 generic presentation can never be interpreted as a v5 Deck by accident.
  const payload = parsePayload(record.payload);
  if (kind !== payload.kind) {
    throw new Error(`Artifact kind 不匹配：${kind} / ${payload.kind}`);
  }
  return { format: "open-office-artifact", schemaVersion, artifactId, revision, kind, payload };
}

function parsePayload(value: unknown): ArtifactPayload {
  const record = asRecord(value, "payload");
  const kind = asKind(record.kind);
  switch (kind) {
    case "document":
      return { kind, data: parseDocumentModel(record.data) };
    case "spreadsheet":
      return { kind, data: parseSpreadsheetModel(record.data) };
    case "presentation":
      return { kind, data: parsePresentationV5Deck(record.data) };
    case "mindmap":
      return { kind, data: parseMindmapModel(record.data) };
    case "whiteboard":
      return { kind, data: parseWhiteboardModel(record.data) };
  }
}

function parseDocumentModel(value: unknown): DocumentModel {
  const record = asRecord(value, "document data");
  const root = asStringArray(record.root, "document root");
  const rawBlocks = asArray(record.blocks, "document blocks");
  const blocks = rawBlocks.map((block, index) => parseBlock(block, index));
  const byId = new Map<string, DocumentBlock>();
  for (const block of blocks) {
    if (byId.has(block.id)) throw new Error(`block id 重复：${block.id}`);
    byId.set(block.id, block);
  }
  const roots = new Set<string>();
  for (const id of root) {
    if (roots.has(id)) throw new Error(`root 子节点重复：${id}`);
    roots.add(id);
    if (!byId.has(id)) throw new Error(`root 引用了不存在的 block：${id}`);
  }
  for (const block of blocks) {
    const children = new Set<string>();
    for (const child of block.children) {
      if (children.has(child)) throw new Error(`子节点重复：${child}`);
      children.add(child);
      if (!byId.has(child)) throw new Error(`block 引用了不存在的子节点：${child}`);
    }
  }
  validateTree(root, byId);
  const pageSetup = record.pageSetup === null ? null : parsePageSetup(record.pageSetup);
  return { root, blocks, pageSetup };
}

function validateTree(root: string[], byId: Map<string, DocumentBlock>): void {
  const visiting = new Set<string>();
  const visited = new Set<string>();
  const owners = new Map<string, string>();
  const visit = (id: string, owner: string): void => {
    if (visiting.has(id)) throw new Error(`Block Tree 存在环：${id}`);
    const previousOwner = owners.get(id);
    if (previousOwner && previousOwner !== owner) {
      throw new Error(`block 被多个父节点引用：${id}`);
    }
    if (visited.has(id)) return;
    owners.set(id, owner);
    visiting.add(id);
    for (const child of byId.get(id)?.children ?? []) visit(child, id);
    visiting.delete(id);
    visited.add(id);
  };
  for (const id of root) visit(id, "<root>");
  if (visited.size !== byId.size) {
    const orphan = [...byId.keys()].find((id) => !visited.has(id));
    throw new Error(`节点不在 root 可达树中：${orphan ?? "unknown"}`);
  }
}

function parseBlock(value: unknown, index: number): DocumentBlock {
  const record = asRecord(value, `block[${index}]`);
  const kind = parseDocumentBlockKind(record.kind, index);
  const content = record.content === null ? null : parseRichText(record.content);
  if (!("presentation" in record)) throw new Error(`block[${index}].presentation 是必需字段`);
  if (!("data" in record)) throw new Error(`block[${index}].data 是必需字段`);
  if ("attrs" in record || "payload" in record) {
    throw new Error(`block[${index}] 不允许使用旧 attrs/payload 字段`);
  }
  const presentation = parseBlockPresentation(record.presentation, `block[${index}].presentation`);
  const data = parseBlockData(record.data, index);
  validateBlockData(kind, data, index);
  return {
    id: asNonEmptyString(record.id, `block[${index}].id`),
    kind,
    presentation,
    content,
    children: asStringArray(record.children, `block[${index}].children`),
    data,
  };
}

function parseBlockPresentation(value: unknown, name: string): BlockPresentation {
  const record = asRecord(value, name);
  assertKnownKeys(record, ["align", "list", "indentStart", "indentEnd", "spacingBefore", "spacingAfter", "lineHeight", "namedStyle"], name);
  const align = record.align === undefined ? "left" : record.align;
  if (align !== "left" && align !== "center" && align !== "right" && align !== "justify") {
    throw new Error(`${name}.align 无效`);
  }
  const list = record.list === undefined || record.list === null ? null : parseListPresentation(record.list, `${name}.list`);
  const indentStart = record.indentStart === undefined ? 0 : asNonNegativeInteger(record.indentStart, `${name}.indentStart`);
  if (indentStart > 20) throw new Error(`${name}.indentStart 必须是 0 到 20 的整数`);
  const indentEnd = record.indentEnd === undefined ? 0 : asBoundedNonNegativeNumber(record.indentEnd, `${name}.indentEnd`);
  const spacingBefore = record.spacingBefore === undefined ? 0 : asBoundedNonNegativeNumber(record.spacingBefore, `${name}.spacingBefore`);
  const spacingAfter = record.spacingAfter === undefined ? 0 : asBoundedNonNegativeNumber(record.spacingAfter, `${name}.spacingAfter`);
  const lineHeight = record.lineHeight === undefined ? 1 : asFiniteNumber(record.lineHeight, `${name}.lineHeight`);
  if (lineHeight < 0.1 || lineHeight > 10) throw new Error(`${name}.lineHeight 必须在 0.1 到 10 之间`);
  const namedStyle = record.namedStyle === undefined || record.namedStyle === null ? null : parseParagraphStyleRef(record.namedStyle, `${name}.namedStyle`);
  return { align, list, indentStart, indentEnd, spacingBefore, spacingAfter, lineHeight, namedStyle };
}

function parseListPresentation(value: unknown, name: string): ListPresentation {
  const record = asRecord(value, name);
  assertKnownKeys(record, ["kind", "level"], name);
  if (record.kind !== "bullet" && record.kind !== "ordered") throw new Error(`${name}.kind 无效`);
  const level = record.level === undefined ? 0 : asNonNegativeInteger(record.level, `${name}.level`);
  if (level > 20) throw new Error(`${name}.level 必须是 0 到 20 的整数`);
  return { kind: record.kind, level };
}

function parseParagraphStyleRef(value: unknown, name: string): ParagraphStyleRef {
  const record = asRecord(value, name);
  assertKnownKeys(record, ["name"], name);
  const styleName = asNonEmptyString(record.name, `${name}.name`);
  if (Array.from(styleName).length > 256) throw new Error(`${name}.name 不能超过 256 个字符`);
  return { name: styleName };
}

function parseBlockData(value: unknown, index: number): BlockData {
  const name = `block[${index}].data`;
  const record = asRecord(value, name);
  const type = asNonEmptyString(record.type, `${name}.type`);
  if (type === "none") {
    assertKnownKeys(record, ["type"], name);
    return { type };
  }
  const data = asRecord(record.data, `${name}.data`);
  if (type === "code") {
    return { type, data: parseCodeBlockConfig(data, `${name}.data`) };
  }
  if (type === "image") {
    const transform = parseImageTransform(data.transform, `${name}.data.transform`);
    const originalAssetId = data.originalAssetId === undefined || data.originalAssetId === null
      ? null
      : asNonEmptyString(data.originalAssetId, `${name}.data.originalAssetId`);
    const caption = data.caption === undefined ? "" : asString(data.caption, `${name}.data.caption`);
    if (Array.from(caption).length > 512) throw new Error(`${name}.data.caption 不能超过 512 个字符`);
    return {
      type,
      data: {
        assetId: asNonEmptyString(data.assetId, `${name}.data.assetId`),
        alt: data.alt === undefined ? "" : asString(data.alt, `${name}.data.alt`),
        originalAssetId,
        transform,
        caption,
      },
    };
  }
  if (type === "todo") {
    return { type, data: { checked: data.checked === undefined ? false : asBoolean(data.checked, `${name}.data.checked`) } };
  }
  if (type === "link") {
    const url = asNonEmptyString(data.url, `${name}.data.url`);
    if (Array.from(url).length > 8192) throw new Error(`block[${index}] link url 不能超过 8192 个字符`);
    return { type, data: { url } };
  }
  if (type === "table") {
    const columns = asArray(data.columns, `${name}.data.columns`).map((item, columnIndex) => {
      const column = asRecord(item, `${name}.data.columns[${columnIndex}]`);
      const width = column.width === undefined || column.width === null
        ? null
        : asFinitePositiveNumber(column.width, `${name}.data.columns[${columnIndex}].width`);
      return {
        id: asNonEmptyString(column.id, `${name}.data.columns[${columnIndex}].id`),
        width,
      };
    });
    if (columns.length === 0) throw new Error(`block[${index}] table 至少需要一列`);
    assertUniqueIds(columns.map((column) => column.id), `block[${index}] table columns`);
    const rows = asArray(data.rows, `${name}.data.rows`).map((item, rowIndex) => {
      const row = asRecord(item, `${name}.data.rows[${rowIndex}]`);
      const cells = asArray(row.cells, `${name}.data.rows[${rowIndex}].cells`)
        .map((cellValue, cellIndex) => {
          const cell = asRecord(cellValue, `${name}.data.rows[${rowIndex}].cells[${cellIndex}]`);
          return {
            id: asNonEmptyString(cell.id, `${name}.data.rows[${rowIndex}].cells[${cellIndex}].id`),
            content: parseRichText(cell.content),
            style: parseTableCellStyle(cell.style),
          };
        });
      if (cells.length !== columns.length) {
        throw new Error(`block[${index}] table row ${row.id ?? rowIndex} 单元格数量与列数不一致`);
      }
      assertUniqueIds(cells.map((cell) => cell.id), `block[${index}] table row cells`);
      const height = row.height === undefined || row.height === null
        ? null
        : asFinitePositiveNumber(row.height, `block[${index}].payload.data.rows[${rowIndex}].height`);
      return { id: asNonEmptyString(row.id, `block[${index}] table row id`), height, cells };
    });
    assertUniqueIds(rows.map((row) => row.id), `block[${index}] table rows`);
    const mergedRanges = asArray(data.mergedRanges, `${name}.data.mergedRanges`).map((value, rangeIndex) => {
      const range = asRecord(value, `${name}.data.mergedRanges[${rangeIndex}]`);
      return {
        startRowId: asNonEmptyString(range.startRowId, "table merge startRowId"),
        endRowId: asNonEmptyString(range.endRowId, "table merge endRowId"),
        startColumnId: asNonEmptyString(range.startColumnId, "table merge startColumnId"),
        endColumnId: asNonEmptyString(range.endColumnId, "table merge endColumnId"),
      };
    });
    validateTableMergedRanges(mergedRanges, rows, columns, `${name}.data.mergedRanges`);
    return { type, data: { columns, rows, mergedRanges } };
  }
  if (type === "extension") {
    assertKnownKeys(record, ["type", "data"], name);
    return {
      type,
      data: {
        typeId: asNonEmptyString(data.typeId, `${name}.data.typeId`),
        raw: data.raw,
      },
    };
  }
  throw new Error(`${name}.type 不支持：${type}`);
}

function validateTableMergedRanges(
  ranges: TableRange[],
  rows: TableRow[],
  columns: TableColumn[],
  name: string,
): void {
  const rowIndex = new Map(rows.map((row, index) => [row.id, index]));
  const columnIndex = new Map(columns.map((column, index) => [column.id, index]));
  const indexed = ranges.map((range, index) => {
    const startRow = rowIndex.get(range.startRowId);
    const endRow = rowIndex.get(range.endRowId);
    const startColumn = columnIndex.get(range.startColumnId);
    const endColumn = columnIndex.get(range.endColumnId);
    if (startRow === undefined || endRow === undefined || startColumn === undefined || endColumn === undefined) {
      throw new Error(`${name}[${index}] 引用了不存在的行或列`);
    }
    if (startRow > endRow || startColumn > endColumn) {
      throw new Error(`${name}[${index}] 必须按文档顺序表示范围`);
    }
    if (startRow === endRow && startColumn === endColumn) {
      throw new Error(`${name}[${index}] 不能只包含一个单元格`);
    }
    return { startRow, endRow, startColumn, endColumn };
  });
  for (let index = 0; index < indexed.length; index += 1) {
    const current = indexed[index];
    for (let previousIndex = 0; previousIndex < index; previousIndex += 1) {
      const previous = indexed[previousIndex];
      const overlaps = current.startRow <= previous.endRow
        && previous.startRow <= current.endRow
        && current.startColumn <= previous.endColumn
        && previous.startColumn <= current.endColumn;
      if (overlaps) throw new Error(`${name}[${index}] 与 ${name}[${previousIndex}] 重叠`);
    }
  }
}

function parseTableCellStyle(value: unknown): TableCellStyle {
  if (value === undefined || value === null) return {};
  const style = asRecord(value, "table cell style");
  const fillColor = style.fillColor === undefined ? undefined : asNonEmptyString(style.fillColor, "table cell fillColor");
  const horizontalAlign = style.horizontalAlign === undefined ? undefined : asTableCellHorizontalAlign(style.horizontalAlign);
  const verticalAlign = style.verticalAlign === undefined ? undefined : asTableCellVerticalAlign(style.verticalAlign);
  const borders = style.borders === undefined ? undefined : parseTableBorderEdges(style.borders);
  return { fillColor, horizontalAlign, verticalAlign, borders };
}

function parseTableBorderEdges(value: unknown): TableBorderEdges {
  const edges = asRecord(value, "table cell borders");
  return {
    top: parseTableBorder(edges.top, "table cell borders.top"),
    right: parseTableBorder(edges.right, "table cell borders.right"),
    bottom: parseTableBorder(edges.bottom, "table cell borders.bottom"),
    left: parseTableBorder(edges.left, "table cell borders.left"),
    diagonalDown: parseTableBorder(edges.diagonalDown, "table cell borders.diagonalDown"),
    diagonalUp: parseTableBorder(edges.diagonalUp, "table cell borders.diagonalUp"),
  };
}

function parseTableBorder(value: unknown, name: string): TableBorder | undefined {
  if (value === undefined || value === null) return undefined;
  const border = asRecord(value, name);
  const style = border.style;
  if (style !== "solid" && style !== "dashed" && style !== "dotted" && style !== "double") {
    throw new Error(`${name}.style 无效`);
  }
  const color = asNonEmptyString(border.color, `${name}.color`);
  if (!/^#[0-9a-fA-F]{6}([0-9a-fA-F]{2})?$/.test(color)) {
    throw new Error(`${name}.color 必须是 #RRGGBB 或 #RRGGBBAA`);
  }
  const width = asFinitePositiveNumber(border.width, `${name}.width`);
  if (width > 32) throw new Error(`${name}.width 不能超过 32`);
  return { style, color, width };
}

function asTableCellHorizontalAlign(value: unknown): TableCellStyle["horizontalAlign"] {
  if (value === "left" || value === "center" || value === "right") return value;
  throw new Error("table cell horizontalAlign 无效");
}

function asTableCellVerticalAlign(value: unknown): TableCellStyle["verticalAlign"] {
  if (value === "top" || value === "middle" || value === "bottom") return value;
  throw new Error("table cell verticalAlign 无效");
}

function parseCodeBlockConfig(data: Record<string, unknown>, name: string): CodeBlockConfig {
  const title = data.title === undefined ? "" : asString(data.title, `${name}.title`);
  if (Array.from(title).length > 256) throw new Error(`${name}.title 不能超过 256 个字符`);
  const language = data.language === undefined
    ? DEFAULT_CODE_BLOCK_CONFIG.language
    : asCodeIdentifier(data.language, `${name}.language`);
  const theme = data.theme === undefined
    ? DEFAULT_CODE_BLOCK_CONFIG.theme
    : asCodeIdentifier(data.theme, `${name}.theme`);
  const height = data.height === undefined
    ? DEFAULT_CODE_BLOCK_CONFIG.height
    : asCodeBlockHeight(data.height, `${name}.height`);
  const showLineNumbers = data.showLineNumbers === undefined
    ? DEFAULT_CODE_BLOCK_CONFIG.showLineNumbers
    : asBoolean(data.showLineNumbers, `${name}.showLineNumbers`);
  const wrap = data.wrap === undefined
    ? DEFAULT_CODE_BLOCK_CONFIG.wrap
    : asBoolean(data.wrap, `${name}.wrap`);
  const indentMode = data.indentMode === undefined
    ? DEFAULT_CODE_BLOCK_CONFIG.indentMode
    : asCodeIndentMode(data.indentMode, `${name}.indentMode`);
  const indentWidth = data.indentWidth === undefined
    ? DEFAULT_CODE_BLOCK_CONFIG.indentWidth
    : asCodeIndentWidth(data.indentWidth, `${name}.indentWidth`);
  const fontSize = data.fontSize === undefined
    ? DEFAULT_CODE_BLOCK_CONFIG.fontSize
    : asCodeFontSize(data.fontSize, `${name}.fontSize`);
  return { title, language, theme, height, showLineNumbers, wrap, indentMode, indentWidth, fontSize };
}

function parseImageTransform(value: unknown, name: string): ImageTransform {
  if (value === undefined || value === null) return defaultImageTransform();
  const transform = asRecord(value, name);
  const cropValue = transform.crop === undefined || transform.crop === null ? {} : asRecord(transform.crop, `${name}.crop`);
  const crop = {
    top: cropValue.top === undefined ? 0 : asFiniteNumber(cropValue.top, `${name}.crop.top`),
    right: cropValue.right === undefined ? 0 : asFiniteNumber(cropValue.right, `${name}.crop.right`),
    bottom: cropValue.bottom === undefined ? 0 : asFiniteNumber(cropValue.bottom, `${name}.crop.bottom`),
    left: cropValue.left === undefined ? 0 : asFiniteNumber(cropValue.left, `${name}.crop.left`),
  };
  for (const [edge, amount] of Object.entries(crop)) {
    if (amount < 0 || amount >= 1) throw new Error(`${name}.crop.${edge} 必须在 0 到 1 之间`);
  }
  if (crop.left + crop.right >= 0.95 || crop.top + crop.bottom >= 0.95) {
    throw new Error(`${name}.crop 裁剪区域不能为空`);
  }
  return {
    crop,
    flipHorizontal: transform.flipHorizontal === undefined ? false : asBoolean(transform.flipHorizontal, `${name}.flipHorizontal`),
    flipVertical: transform.flipVertical === undefined ? false : asBoolean(transform.flipVertical, `${name}.flipVertical`),
  };
}

function validateBlockData(
  kind: DocumentBlockKind,
  data: BlockData,
  index: number,
): void {
  const kindType = kind.type;
  const knownStructural = new Set(["paragraph", "heading", "quote", "code", "image", "table", "callout", "todo", "divider", "page", "columns", "column", "link", "extension"]);
  if (kindType === "image" && data.type !== "image") {
    throw new Error(`block[${index}] image 必须包含 image data`);
  }
  if (kindType === "table" && data.type !== "table") {
    throw new Error(`block[${index}] table 必须包含 table data`);
  }
  if (kindType === "code") {
    if (data.type !== "code") {
      throw new Error(`block[${index}] code 必须包含 code data`);
    }
  }
  if (kindType === "todo" && data.type !== "todo") {
    throw new Error(`block[${index}] todo 必须包含 todo data`);
  }
  if (kindType === "link" && data.type !== "link") {
    throw new Error(`block[${index}] link 必须包含 link data`);
  }
  if (kindType === "extension" && (data.type !== "extension" || data.data.typeId !== kind.typeId)) {
    throw new Error(`block[${index}] extension 必须包含匹配的 extension data`);
  }
  if (!knownStructural.has(kindType) && (data.type !== "extension" || data.data.typeId !== kindType)) {
    throw new Error(`block[${index}] unknown block 必须包含匹配的 extension data`);
  }
  if (kindType === "image" && data.type === "image" && !data.data.assetId.trim()) {
    throw new Error(`block[${index}] image assetId 不能为空`);
  }
  if (knownStructural.has(kindType) && kindType !== "image" && kindType !== "table" && kindType !== "code" && kindType !== "todo" && kindType !== "link" && kindType !== "extension" && data.type !== "none") {
    throw new Error(`block[${index}] 文本 block 不允许携带结构化 data`);
  }
}

function parseDocumentBlockKind(value: unknown, index: number): DocumentBlockKind {
  const record = asRecord(value, `block[${index}].kind`);
  const type = asNonEmptyString(record.type, `block[${index}].kind.type`);
  switch (type) {
    case "paragraph":
    case "quote":
    case "code":
    case "image":
    case "table":
    case "callout":
    case "todo":
    case "divider":
    case "page":
    case "columns":
    case "column":
    case "link":
      return { type };
    case "heading": {
      const level = asPositiveInteger(record.level, `block[${index}].kind.level`);
      if (level > 6) throw new Error(`block[${index}] heading level 必须在 1 到 6 之间`);
      return { type, level };
    }
    case "extension":
      return { type, typeId: asNonEmptyString(record.typeId, `block[${index}].kind.typeId`) };
    default:
      // Preserve every unknown field. The Rust schema uses the same envelope when it writes it
      // back, so a client that does not understand a future block remains lossless.
      return { ...record, type };
  }
}

function parseRichText(value: unknown): RichText {
  const record = asRecord(value, "rich text");
  const text = asString(record.text, "rich text text");
  const textLength = Array.from(text).length;
  const runs = asArray(record.runs, "rich text runs").map((run, index) => {
    const item = asRecord(run, `run[${index}]`);
    if (!("style" in item)) throw new Error(`run[${index}].style 是必需字段；旧 attrs 不再属于在线 schema`);
    const style = parseInlineStyle(item.style, `run[${index}].style`);
    return {
      start: asNonNegativeInteger(item.start, `run[${index}].start`),
      end: asPositiveInteger(item.end, `run[${index}].end`),
      style,
    };
  });
  let expectedStart = 0;
  for (const [index, run] of runs.entries()) {
    if (run.start !== expectedStart || run.start >= run.end || run.end > textLength) {
      throw new Error(`run[${index}] 区间无效：${run.start}..${run.end}，文本长度 ${textLength}`);
    }
    expectedStart = run.end;
  }
  if (runs.length > 0 && expectedStart !== textLength) {
    throw new Error("rich text runs 未覆盖全文");
  }
  return {
    text,
    runs,
  };
}

function parseInlineStyle(value: unknown, name: string): InlineStyle {
  const style = asRecord(value ?? {}, name);
  const known = new Set(["bold", "italic", "underline", "strikethrough", "fontFamily", "fontSize", "color", "highlight", "verticalAlign"]);
  for (const key of Object.keys(style)) {
    if (!known.has(key)) throw new Error(`${name}.${key} 不是受支持的行内样式字段`);
  }
  for (const key of ["bold", "italic", "underline", "strikethrough"] as const) {
    if (style[key] !== undefined && typeof style[key] !== "boolean") throw new Error(`${name}.${key} 必须是布尔值`);
  }
  const fontSize = style.fontSize === undefined || style.fontSize === null ? null : style.fontSize;
  if (fontSize !== null
    && (typeof fontSize !== "number" || !Number.isFinite(fontSize)
      || fontSize <= 0 || fontSize > 512)) {
    throw new Error(`${name}.fontSize 必须在 0 到 512 之间`);
  }
  for (const key of ["fontFamily", "color", "highlight"] as const) {
    if (style[key] !== undefined && style[key] !== null && (typeof style[key] !== "string" || !style[key].trim())) {
      throw new Error(`${name}.${key} 必须是非空字符串`);
    }
  }
  if (style.color !== undefined && style.color !== null && typeof style.color === "string" && !isStrictColor(style.color)) throw new Error(`${name}.color 颜色 token 无效`);
  if (style.highlight !== undefined && style.highlight !== null && typeof style.highlight === "string" && !isStrictColor(style.highlight)) throw new Error(`${name}.highlight 颜色 token 无效`);
  if (style.verticalAlign !== undefined && style.verticalAlign !== null
    && style.verticalAlign !== "baseline" && style.verticalAlign !== "superscript" && style.verticalAlign !== "subscript") {
    throw new Error(`${name}.verticalAlign 无效`);
  }
  return {
    bold: style.bold === true,
    italic: style.italic === true,
    underline: style.underline === true,
    strikethrough: style.strikethrough === true,
    fontFamily: style.fontFamily === undefined || style.fontFamily === null ? null : style.fontFamily as string,
    fontSize: fontSize as number | null,
    color: style.color === undefined || style.color === null ? null : style.color as string,
    highlight: style.highlight === undefined || style.highlight === null ? null : style.highlight as string,
    verticalAlign: style.verticalAlign === undefined || style.verticalAlign === null ? null : style.verticalAlign as VerticalAlign,
  };
}

function isStrictColor(value: string): boolean {
  const trimmed = value.trim();
  if (/^#[0-9a-fA-F]{3,4}$/.test(trimmed) || /^#[0-9a-fA-F]{6}(?:[0-9a-fA-F]{2})?$/.test(trimmed)) return true;
  const match = trimmed.match(/^rgba?\(([^)]+)\)$/i);
  if (!match) return false;
  const parts = match[1].split(",").map((part) => part.trim());
  if ((trimmed.toLowerCase().startsWith("rgb(") && parts.length !== 3)
    || (trimmed.toLowerCase().startsWith("rgba(") && parts.length !== 4)) return false;
  if (parts.slice(0, 3).some((part) => !/^\d+$/.test(part) || Number(part) > 255)) return false;
  return parts.length === 3 || (/^\d*\.?\d+$/.test(parts[3]) && Number(parts[3]) >= 0 && Number(parts[3]) <= 1);
}

function parsePageSetup(value: unknown): ArtifactPageSetup {
  const record = asRecord(value, "pageSetup");
  const pageSetup = {
    width: asFiniteNumber(record.width, "pageSetup.width"),
    height: asFiniteNumber(record.height, "pageSetup.height"),
    marginTop: asFiniteNumber(record.marginTop, "pageSetup.marginTop"),
    marginRight: asFiniteNumber(record.marginRight, "pageSetup.marginRight"),
    marginBottom: asFiniteNumber(record.marginBottom, "pageSetup.marginBottom"),
    marginLeft: asFiniteNumber(record.marginLeft, "pageSetup.marginLeft"),
  };
  if (pageSetup.width <= 0 || pageSetup.height <= 0) {
    throw new Error("pageSetup 宽高必须大于 0");
  }
  if ([pageSetup.marginTop, pageSetup.marginRight, pageSetup.marginBottom, pageSetup.marginLeft]
    .some((margin) => margin < 0)) {
    throw new Error("pageSetup 页边距不能为负数");
  }
  if (pageSetup.marginLeft + pageSetup.marginRight >= pageSetup.width
    || pageSetup.marginTop + pageSetup.marginBottom >= pageSetup.height) {
    throw new Error("pageSetup 页边距必须小于页面尺寸");
  }
  return pageSetup;
}

function parseSpreadsheetModel(value: unknown): SpreadsheetModel {
  const record = asRecord(value, "spreadsheet data");
  const metadata = parseSpreadsheetMetadata(record.metadata, "spreadsheet.metadata");
  const rawSheets = asArray(record.sheets, "spreadsheet sheets");
  const ids = new Set<string>();
  const sheets = rawSheets.map((rawSheet, index) => {
    const sheet = asRecord(rawSheet, `sheet[${index}]`);
    const id = asNonEmptyString(sheet.id, `sheet[${index}].id`);
    if (ids.has(id)) throw new Error(`sheet id 重复：${id}`);
    ids.add(id);
    const name = asNonEmptyString(sheet.name, `sheet[${index}].name`);
    const cells = asArray(sheet.cells, `sheet[${index}].cells`).map((rawCell, cellIndex) => {
      const cell = asRecord(rawCell, `sheet[${index}].cell[${cellIndex}]`);
      const row = asNonNegativeInteger(cell.row, `sheet[${index}].cell[${cellIndex}].row`);
      const column = asNonNegativeInteger(cell.column, `sheet[${index}].cell[${cellIndex}].column`);
      const attrs = asRecord(cell.attrs, `sheet[${index}].cell[${cellIndex}].attrs`);
      return {
        row,
        column,
        ...(cell.value === undefined ? {} : { value: cell.value }),
        ...(cell.formula === undefined
          ? {}
          : { formula: asString(cell.formula, `sheet[${index}].cell[${cellIndex}].formula`) }),
        attrs,
        ...(cell.style === undefined ? {} : { style: parseCellStyle(cell.style, `sheet[${index}].cell[${cellIndex}].style`) }),
      } satisfies CellModel;
    });
    const coordinates = new Set(cells.map((cell) => `${cell.row}:${cell.column}`));
    if (coordinates.size !== cells.length) {
      throw new Error(`sheet[${index}] 存在重复 cell 坐标`);
    }
    return {
      id,
      name,
      cells,
      metadata: parseSheetMetadata(sheet.metadata, `sheet[${index}].metadata`),
    } satisfies SheetModel;
  });
  if (metadata.activeSheetId !== null && !ids.has(metadata.activeSheetId)) {
    throw new Error(`spreadsheet.metadata.activeSheetId 不存在：${metadata.activeSheetId}`);
  }
  return { metadata, sheets };
}

function parseSpreadsheetMetadata(value: unknown, name: string): SpreadsheetMetadata {
  if (value === undefined || value === null) {
    return { activeSheetId: null, calculationMode: "automatic", dateSystem: "excel1900" };
  }
  const record = asRecord(value, name);
  const activeSheetId = record.activeSheetId === undefined || record.activeSheetId === null
    ? null
    : asNonEmptyString(record.activeSheetId, `${name}.activeSheetId`);
  const calculationMode = record.calculationMode === undefined ? "automatic" : record.calculationMode;
  if (calculationMode !== "automatic" && calculationMode !== "manual") throw new Error(`${name}.calculationMode 无效`);
  const dateSystem = record.dateSystem === undefined ? "excel1900" : record.dateSystem;
  if (dateSystem !== "excel1900" && dateSystem !== "excel1904") throw new Error(`${name}.dateSystem 无效`);
  return { activeSheetId, calculationMode, dateSystem };
}

function parseSheetMetadata(value: unknown, name: string): SheetMetadata {
  if (value === undefined || value === null) return defaultSheetMetadata();
  const record = asRecord(value, name);
  const visibility = record.visibility === undefined ? "visible" : record.visibility;
  if (visibility !== "visible" && visibility !== "hidden" && visibility !== "veryHidden") throw new Error(`${name}.visibility 无效`);
  const freeze = record.freeze === undefined ? { rows: 0, columns: 0 } : parseFreezePane(record.freeze, `${name}.freeze`);
  const rowCount = record.rowCount === undefined || record.rowCount === null ? null : asNonNegativeInteger(record.rowCount, `${name}.rowCount`);
  const columnCount = record.columnCount === undefined || record.columnCount === null ? null : asNonNegativeInteger(record.columnCount, `${name}.columnCount`);
  if (rowCount !== null && freeze.rows > rowCount || columnCount !== null && freeze.columns > columnCount) throw new Error(`${name}.freeze 超出 worksheet 边界`);
  const mergedRanges = parseRanges(record.mergedRanges, `${name}.mergedRanges`);
  const conditionalFormats = asArray(record.conditionalFormats ?? [], `${name}.conditionalFormats`).map((item, index) => parseConditionalFormat(item, `${name}.conditionalFormats[${index}]`));
  const dataValidations = asArray(record.dataValidations ?? [], `${name}.dataValidations`).map((item, index) => parseDataValidation(item, `${name}.dataValidations[${index}]`));
  const autoFilter = record.autoFilter === undefined || record.autoFilter === null ? null : parseFilter(record.autoFilter, `${name}.autoFilter`);
  const sort = record.sort === undefined || record.sort === null ? null : parseSort(record.sort, `${name}.sort`);
  const media = asArray(record.media ?? [], `${name}.media`).map((item, index) => parseMedia(item, `${name}.media[${index}]`));
  return { visibility, rowCount, columnCount, freeze, autoFilter, sort, conditionalFormats, dataValidations, mergedRanges, media };
}

function defaultSheetMetadata(): SheetMetadata {
  return { visibility: "visible", rowCount: null, columnCount: null, freeze: { rows: 0, columns: 0 }, autoFilter: null, sort: null, conditionalFormats: [], dataValidations: [], mergedRanges: [], media: [] };
}

function parseFreezePane(value: unknown, name: string): FreezePane {
  const record = asRecord(value, name);
  return { rows: asNonNegativeInteger(record.rows ?? 0, `${name}.rows`), columns: asNonNegativeInteger(record.columns ?? 0, `${name}.columns`) };
}

function parseGridRange(value: unknown, name: string): GridRange {
  const record = asRecord(value, name);
  const range = {
    startRow: asNonNegativeInteger(record.startRow, `${name}.startRow`),
    startColumn: asNonNegativeInteger(record.startColumn, `${name}.startColumn`),
    endRow: asNonNegativeInteger(record.endRow, `${name}.endRow`),
    endColumn: asNonNegativeInteger(record.endColumn, `${name}.endColumn`),
  };
  if (range.startRow > range.endRow || range.startColumn > range.endColumn) throw new Error(`${name} 起点不得超过终点`);
  return range;
}

function parseRanges(value: unknown, name: string): GridRange[] {
  return asArray(value ?? [], name).map((item, index) => parseGridRange(item, `${name}[${index}]`));
}

function parseFilter(value: unknown, name: string): FilterSpec {
  const record = asRecord(value, name);
  const range = parseGridRange(record.range, `${name}.range`);
  const columns = asArray(record.columns ?? [], `${name}.columns`).map((item, index) => {
    const column = asRecord(item, `${name}.columns[${index}]`);
    return { column: asNonNegativeInteger(column.column, `${name}.columns[${index}].column`), predicate: parseFilterPredicate(column.predicate, `${name}.columns[${index}].predicate`) };
  });
  if (columns.some((column) => column.column < range.startColumn || column.column > range.endColumn)) throw new Error(`${name}.columns 超出 range`);
  return { range, columns };
}

function parseFilterPredicate(value: unknown, name: string): FilterPredicate {
  const record = asRecord(value, name);
  if (record.type === "values") return { type: "values", value: asArray(record.value, `${name}.value`) };
  if (record.type === "contains") return { type: "contains", value: asString(record.value, `${name}.value`) };
  if (record.type === "equals") return { type: "equals", value: record.value };
  if (record.type === "greaterThan" || record.type === "lessThan") return { type: record.type, value: asFiniteNumber(record.value, `${name}.value`) };
  throw new Error(`${name}.type 无效`);
}

function parseSort(value: unknown, name: string): SortSpec {
  const record = asRecord(value, name);
  const range = parseGridRange(record.range, `${name}.range`);
  const keys = asArray(record.keys ?? [], `${name}.keys`).map((item, index) => {
    const key = asRecord(item, `${name}.keys[${index}]`);
    const direction = key.direction === "descending" ? "descending" : key.direction === "ascending" ? "ascending" : (() => { throw new Error(`${name}.keys[${index}].direction 无效`); })();
    return { column: asNonNegativeInteger(key.column, `${name}.keys[${index}].column`), direction } satisfies SortKey;
  });
  if (keys.some((key) => key.column < range.startColumn || key.column > range.endColumn)) throw new Error(`${name}.keys 超出 range`);
  return { range, keys };
}

function parseConditionalFormat(value: unknown, name: string): ConditionalFormatRule {
  const record = asRecord(value, name);
  return { id: asNonEmptyString(record.id, `${name}.id`), range: parseGridRange(record.range, `${name}.range`), predicate: parseConditionalPredicate(record.predicate, `${name}.predicate`), style: parseCellStyle(record.style, `${name}.style`) };
}

function parseConditionalPredicate(value: unknown, name: string): ConditionalPredicate {
  const record = asRecord(value, name);
  if (record.type === "formula") return { type: "formula", value: asString(record.value, `${name}.value`) };
  if (record.type === "colorScale") { const data = asRecord(record.value, `${name}.value`); return { type: "colorScale", value: { min: asNonEmptyString(data.min, `${name}.value.min`), max: asNonEmptyString(data.max, `${name}.value.max`) } }; }
  if (record.type === "cellIs") { const data = asRecord(record.value, `${name}.value`); const operators = ["equal", "notEqual", "greaterThan", "greaterThanOrEqual", "lessThan", "lessThanOrEqual"] as const; if (!operators.includes(data.operator as typeof operators[number])) throw new Error(`${name}.value.operator 无效`); return { type: "cellIs", value: { operator: data.operator as ComparisonOperator, value: data.value } }; }
  throw new Error(`${name}.type 无效`);
}

function parseDataValidation(value: unknown, name: string): DataValidationRule {
  const record = asRecord(value, name);
  const kind = asRecord(record.kind, `${name}.kind`);
  const type = kind.type;
  let parsed: DataValidationKind;
  if (type === "list") parsed = { type: "list", value: asArray(kind.value, `${name}.kind.value`).map((item, index) => asString(item, `${name}.kind.value[${index}]`)) };
  else if (type === "wholeNumber" || type === "decimal") { const data = asRecord(kind.value, `${name}.kind.value`); parsed = { type, value: { min: asFiniteNumber(data.min, `${name}.kind.value.min`), max: asFiniteNumber(data.max, `${name}.kind.value.max`) } } as DataValidationKind; }
  else if (type === "date") { const data = asRecord(kind.value, `${name}.kind.value`); parsed = { type: "date", value: { minSerial: asFiniteNumber(data.minSerial, `${name}.kind.value.minSerial`), maxSerial: asFiniteNumber(data.maxSerial, `${name}.kind.value.maxSerial`) } }; }
  else if (type === "customFormula") parsed = { type: "customFormula", value: asString(kind.value, `${name}.kind.value`) };
  else throw new Error(`${name}.kind.type 无效`);
  return { id: asNonEmptyString(record.id, `${name}.id`), range: parseGridRange(record.range, `${name}.range`), kind: parsed, allowBlank: record.allowBlank === undefined ? false : asBoolean(record.allowBlank, `${name}.allowBlank`), errorMessage: record.errorMessage === undefined || record.errorMessage === null ? null : asString(record.errorMessage, `${name}.errorMessage`) };
}

function parseCellStyle(value: unknown, name: string): CellStyle {
  if (value === undefined || value === null) return { numberFormat: null, font: null, fill: null, alignment: null };
  const record = asRecord(value, name);
  const font = record.font === undefined || record.font === null ? null : asRecord(record.font, `${name}.font`);
  const fill = record.fill === undefined || record.fill === null ? null : asRecord(record.fill, `${name}.fill`);
  const alignment = record.alignment === undefined || record.alignment === null ? null : asRecord(record.alignment, `${name}.alignment`);
  return {
    numberFormat: record.numberFormat === undefined || record.numberFormat === null ? null : asString(record.numberFormat, `${name}.numberFormat`),
    font: font === null ? null : { family: font.family === undefined || font.family === null ? null : asString(font.family, `${name}.font.family`), size: font.size === undefined || font.size === null ? null : asFiniteNumber(font.size, `${name}.font.size`), bold: font.bold === undefined ? false : asBoolean(font.bold, `${name}.font.bold`), italic: font.italic === undefined ? false : asBoolean(font.italic, `${name}.font.italic`), color: font.color === undefined || font.color === null ? null : asString(font.color, `${name}.font.color`) },
    fill: fill === null ? null : { foreground: fill.foreground === undefined || fill.foreground === null ? null : asString(fill.foreground, `${name}.fill.foreground`), background: fill.background === undefined || fill.background === null ? null : asString(fill.background, `${name}.fill.background`) },
    alignment: alignment === null ? null : { horizontal: alignment.horizontal === undefined || alignment.horizontal === null ? null : asString(alignment.horizontal, `${name}.alignment.horizontal`), vertical: alignment.vertical === undefined || alignment.vertical === null ? null : asString(alignment.vertical, `${name}.alignment.vertical`), wrap: alignment.wrap === undefined ? false : asBoolean(alignment.wrap, `${name}.alignment.wrap`) },
  };
}

function parseMedia(value: unknown, name: string): SheetMedia {
  const record = asRecord(value, name);
  return { id: asNonEmptyString(record.id, `${name}.id`), relationship: asNonEmptyString(record.relationship, `${name}.relationship`), contentType: asNonEmptyString(record.contentType, `${name}.contentType`), target: asNonEmptyString(record.target, `${name}.target`), anchor: parseGridRange(record.anchor, `${name}.anchor`) };
}

function parseMindmapModel(value: unknown): MindmapModel {
  const record = asRecord(value, "mindmap data");
  const root = record.root === null ? null : asNonEmptyString(record.root, "mindmap root");
  const rawNodes = asArray(record.nodes, "mindmap nodes");
  const ids = new Set<string>();
  const nodes = rawNodes.map((rawNode, index) => {
    const node = asRecord(rawNode, `mindmap node[${index}]`);
    const id = asNonEmptyString(node.id, `mindmap node[${index}].id`);
    if (ids.has(id)) throw new Error(`mindmap node id 重复：${id}`);
    ids.add(id);
    return {
      id,
      parentId: node.parentId === null
        ? null
        : asNonEmptyString(node.parentId, `mindmap node[${index}].parentId`),
      content: node.content === null ? null : parseRichText(node.content),
      attrs: asRecord(node.attrs, `mindmap node[${index}].attrs`),
      collapsed: node.collapsed === true,
    } satisfies MindmapNode;
  });
  const rawEdges = record.edges === undefined ? [] : asArray(record.edges, "mindmap edges");
  if (nodes.length === 0) {
    if (root !== null) throw new Error("空 mindmap 不能设置 root");
    if (rawEdges.length > 0) throw new Error("空 mindmap 不能设置 edge");
    return { root, nodes, edges: [] };
  }
  if (root === null || !ids.has(root)) throw new Error("非空 mindmap 必须引用存在的 root");
  const byId = new Map(nodes.map((node) => [node.id, node]));
  const rootNode = byId.get(root);
  if (rootNode?.parentId !== null) throw new Error("mindmap root 不能有 parentId");
  for (const node of nodes) {
    if (node.parentId === node.id) throw new Error(`mindmap 存在自环：${node.id}`);
    if (node.parentId !== null && !byId.has(node.parentId)) {
      throw new Error(`mindmap node ${node.id} 引用了不存在的 parent：${node.parentId}`);
    }
    if (node.parentId === null && node.id !== root) {
      throw new Error(`mindmap node ${node.id} 缺少 parentId`);
    }
  }
  validateMindmapParents(nodes);
  const edgeIds = new Set<string>();
  const edges = rawEdges.map((rawEdge, index) => {
    const edge = asRecord(rawEdge, `mindmap edge[${index}]`);
    const id = asNonEmptyString(edge.id, `mindmap edge[${index}].id`);
    if (edgeIds.has(id)) throw new Error(`mindmap edge id 重复：${id}`);
    edgeIds.add(id);
    const sourceId = asNonEmptyString(edge.sourceId, `mindmap edge[${index}].sourceId`);
    const targetId = asNonEmptyString(edge.targetId, `mindmap edge[${index}].targetId`);
    if (sourceId === targetId) throw new Error(`mindmap edge ${id} 不能连接自身`);
    if (!ids.has(sourceId) || !ids.has(targetId)) {
      throw new Error(`mindmap edge ${id} 引用了不存在的节点`);
    }
    return { id, sourceId, targetId, attrs: asRecord(edge.attrs, `mindmap edge[${index}].attrs`) } satisfies MindmapEdge;
  });
  return { root, nodes, edges };
}

function validateMindmapParents(nodes: MindmapNode[]): void {
  const byId = new Map(nodes.map((node) => [node.id, node]));
  const visiting = new Set<string>();
  const visited = new Set<string>();
  const visit = (id: string): void => {
    if (visiting.has(id)) throw new Error(`mindmap 存在环：${id}`);
    if (visited.has(id)) return;
    visiting.add(id);
    const parentId = byId.get(id)?.parentId;
    if (parentId !== null && parentId !== undefined) visit(parentId);
    visiting.delete(id);
    visited.add(id);
  };
  for (const node of nodes) visit(node.id);
}

function parseWhiteboardModel(value: unknown): WhiteboardModel {
  const record = asRecord(value, "whiteboard data");
  const cameraRecord = asRecord(record.camera, "whiteboard camera");
  const camera = {
    x: asFiniteNumber(cameraRecord.x, "whiteboard camera.x"),
    y: asFiniteNumber(cameraRecord.y, "whiteboard camera.y"),
    scale: asFiniteNumber(cameraRecord.scale, "whiteboard camera.scale"),
  };
  if (camera.scale <= 0) throw new Error("whiteboard camera.scale 必须大于 0");
  return {
    elements: parseSceneElements(record.elements, "whiteboard elements"),
    camera,
  };
}

function parseSceneElements(value: unknown, name: string): SceneElement[] {
  const rawElements = asArray(value, name);
  const ids = new Set<string>();
  const elements = rawElements.map((rawElement, index) => {
    const element = asRecord(rawElement, `${name}[${index}]`);
    const id = asNonEmptyString(element.id, `${name}[${index}].id`);
    if (ids.has(id)) throw new Error(`scene element id 重复：${id}`);
    ids.add(id);
    const typeId = asNonEmptyString(element.typeId, `${name}[${index}].typeId`);
    const transformRecord = asRecord(element.transform, `${name}[${index}].transform`);
    const transform = {
      x: asFiniteNumber(transformRecord.x, `${name}[${index}].transform.x`),
      y: asFiniteNumber(transformRecord.y, `${name}[${index}].transform.y`),
      width: asFiniteNumber(transformRecord.width, `${name}[${index}].transform.width`),
      height: asFiniteNumber(transformRecord.height, `${name}[${index}].transform.height`),
      rotation: asFiniteNumber(transformRecord.rotation, `${name}[${index}].transform.rotation`),
    };
    if (transform.width < 0 || transform.height < 0) {
      throw new Error(`${name}[${index}].transform 宽高不能为负数`);
    }
    return {
      id,
      typeId,
      transform,
      attrs: asRecord(element.attrs, `${name}[${index}].attrs`),
      children: asStringArray(element.children, `${name}[${index}].children`),
    } satisfies SceneElement;
  });
  const byId = new Map(elements.map((element) => [element.id, element]));
  for (const element of elements) {
    const children = new Set<string>();
    for (const child of element.children) {
      if (children.has(child)) throw new Error(`scene element 子节点重复：${child}`);
      children.add(child);
      if (!byId.has(child)) throw new Error(`scene element 引用了不存在的子节点：${child}`);
    }
  }
  const visiting = new Set<string>();
  const visited = new Set<string>();
  const visit = (id: string): void => {
    if (visiting.has(id)) throw new Error(`scene graph 存在环：${id}`);
    if (visited.has(id)) return;
    visiting.add(id);
    for (const child of byId.get(id)?.children ?? []) visit(child);
    visiting.delete(id);
    visited.add(id);
  };
  for (const element of elements) visit(element.id);
  return elements;
}

function asKind(value: unknown): ArtifactKind {
  if (
    value === "document" ||
    value === "spreadsheet" ||
    value === "presentation" ||
    value === "mindmap" ||
    value === "whiteboard"
  ) {
    return value;
  }
  throw new Error("Artifact kind 无效");
}

function asRecord(value: unknown, name: string): Record<string, unknown> {
  if (typeof value !== "object" || value === null || Array.isArray(value)) {
    throw new Error(`${name} 必须是对象`);
  }
  return value as Record<string, unknown>;
}

function assertKnownKeys(record: Record<string, unknown>, allowed: readonly string[], name: string): void {
  const known = new Set(allowed);
  for (const key of Object.keys(record)) {
    if (!known.has(key)) throw new Error(`${name}.${key} 不是受支持的字段`);
  }
}

function asArray(value: unknown, name: string): unknown[] {
  if (!Array.isArray(value)) throw new Error(`${name} 必须是数组`);
  return value;
}

function asString(value: unknown, name: string): string {
  if (typeof value !== "string") throw new Error(`${name} 必须是字符串`);
  return value;
}

function asBoolean(value: unknown, name: string): boolean {
  if (typeof value !== "boolean") throw new Error(`${name} 必须是布尔值`);
  return value;
}

function asCodeIdentifier(value: unknown, name: string): string {
  const identifier = asNonEmptyString(value, name);
  if (Array.from(identifier).length > 64 || /\s/u.test(identifier)) {
    throw new Error(`${name} 必须是 1 到 64 个不含空白的字符`);
  }
  return identifier;
}

function asCodeIndentMode(value: unknown, name: string): CodeIndentMode {
  if (value !== "spaces" && value !== "tabs") throw new Error(`${name} 必须是 spaces 或 tabs`);
  return value;
}

function asCodeIndentWidth(value: unknown, name: string): 2 | 4 | 8 {
  if (value !== 2 && value !== 4 && value !== 8) throw new Error(`${name} 必须是 2、4 或 8`);
  return value;
}

function asCodeFontSize(value: unknown, name: string): number {
  if (typeof value !== "number" || !Number.isInteger(value) || value < 8 || value > 32) {
    throw new Error(`${name} 必须是 8 到 32 的整数`);
  }
  return value;
}

function asCodeBlockHeight(value: unknown, name: string): number {
  if (typeof value !== "number" || !Number.isInteger(value) || value < 160 || value > 640) {
    throw new Error(`${name} 必须是 160 到 640 的整数`);
  }
  return value;
}

function asNonEmptyString(value: unknown, name: string): string {
  const string = asString(value, name);
  if (!string.trim()) throw new Error(`${name} 不能为空`);
  return string;
}

function asStringArray(value: unknown, name: string): string[] {
  return asArray(value, name).map((item, index) => asString(item, `${name}[${index}]`));
}

function asFiniteNumber(value: unknown, name: string): number {
  if (typeof value !== "number" || !Number.isFinite(value)) {
    throw new Error(`${name} 必须是有限数字`);
  }
  return value;
}

function asFinitePositiveNumber(value: unknown, name: string): number {
  const number = asFiniteNumber(value, name);
  if (number <= 0) throw new Error(`${name} 必须是正数`);
  return number;
}

function asBoundedNonNegativeNumber(value: unknown, name: string): number {
  const number = asFiniteNumber(value, name);
  if (number < 0 || number > 1000) throw new Error(`${name} 必须在 0 到 1000 之间`);
  return number;
}

function assertUniqueIds(ids: string[], name: string): void {
  const unique = new Set(ids);
  if (unique.size !== ids.length) throw new Error(`${name} id 重复`);
}

function asNonNegativeInteger(value: unknown, name: string): number {
  const number = asFiniteNumber(value, name);
  if (!Number.isSafeInteger(number) || number < 0) throw new Error(`${name} 必须是非负整数`);
  return number;
}

function asPositiveInteger(value: unknown, name: string): number {
  const number = asNonNegativeInteger(value, name);
  if (number === 0) throw new Error(`${name} 必须大于 0`);
  return number;
}
