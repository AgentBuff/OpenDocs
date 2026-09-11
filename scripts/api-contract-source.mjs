/**
 * The HTTP contract has one executable source.  Rust owns runtime validation;
 * this source describes the public transport surface and is intentionally
 * limited to DTO shape, methods and headers so generated docs cannot invent a
 * second domain model.
 */

import { readFile } from "node:fs/promises";

const json = (schema) => ({ "content": { "application/json": { schema } } });
const ref = (name) => ({ "$ref": `#/components/schemas/${name}` });
const artifactId = {
  name: "id",
  in: "path",
  required: true,
  schema: { type: "string", minLength: 1 },
};
const blockId = {
  name: "blockId",
  in: "path",
  required: true,
  schema: { type: "string", minLength: 1 },
};
const version = {
  name: "version",
  in: "path",
  required: true,
  schema: { type: "integer", minimum: 1 },
};
const ifMatch = {
  name: "If-Match",
  in: "header",
  required: true,
  description: "Quoted snapshot revision; must equal baseRevision.",
  schema: { type: "string", pattern: '^\\"[0-9]+\\"$' },
};
const transactionId = {
  name: "x-transaction-id",
  in: "header",
  required: true,
  description: "Idempotency key; must equal envelope.transactionId.",
  schema: { type: "string", minLength: 1, maxLength: 128 },
};

const artifactResponse = { description: "Artifact metadata", ...json(ref("ArtifactMeta")) };
const commitResponse = { description: "Committed semantic transaction", ...json(ref("CommitResult")) };
const errorResponse = { description: "Stable machine-readable error", ...json(ref("ErrorEnvelope")) };

// ADR-0010 phase 2: protocol DTO shapes come from the Rust typed contract.
// The golden snapshot is produced by `oo_protocol::generate_contract_schemas`
// and locked by a cargo test; this file only flattens its per-type $defs so
// internal refs resolve against the OpenAPI components root. Hand-written
// schemas below describe server-owned transport DTOs that do not exist in
// oo-protocol yet.
const protocolSnapshotPath = new URL("../crates/oo-protocol/tests/snapshots/contract_schemas.json", import.meta.url);
const rawProtocolSchemas = JSON.parse(await readFile(protocolSnapshotPath, "utf8"));

function rewriteRefs(value) {
  if (Array.isArray(value)) return value.map(rewriteRefs);
  if (value && typeof value === "object") {
    const out = {};
    for (const [key, item] of Object.entries(value)) {
      out[key] = key === "$ref" && typeof item === "string" && item.startsWith("#/$defs/")
        ? `#/components/schemas/${item.slice("#/$defs/".length)}`
        : rewriteRefs(item);
    }
    return out;
  }
  return value;
}

const protocolSchemas = (() => {
  const out = {};
  // Top-level entries embed a $schema keyword and duplicate their referenced
  // types under per-entry $defs; both are normalized away so components hold
  // one canonical copy of every protocol type.
  const put = (name, value) => {
    const encoded = JSON.stringify(value);
    if (out[name]) {
      if (JSON.stringify(out[name]) !== encoded) {
        throw new Error(`conflicting protocol schema: ${name}`);
      }
      return;
    }
    out[name] = value;
  };
  const stripMeta = (value) => {
    const { $schema, ...rest } = value;
    // The components key already carries the type name; schemars only emits
    // `title` on standalone roots, which would otherwise break dedup.
    delete rest.title;
    return rest;
  };
  for (const [name, schema] of Object.entries(rawProtocolSchemas)) {
    const { $defs = {}, ...rest } = schema;
    put(name, rewriteRefs(stripMeta(rest)));
    for (const [defName, def] of Object.entries($defs)) {
      put(defName, rewriteRefs(stripMeta(def)));
    }
  }
  return out;
})();

export const schemas = {
  CollaboratorList: {
    type: "object",
    required: ["collaborators"],
    properties: { collaborators: { type: "array", items: ref("Collaborator") } },
    additionalProperties: false,
  },
  Collaborator: {
    type: "object",
    required: ["userId", "role", "createdAt"],
    properties: {
      userId: { type: "string" },
      role: { enum: ["editor", "viewer"] },
      createdAt: { type: "string", format: "date-time" },
    },
    additionalProperties: false,
  },
  UpsertCollaboratorRequest: {
    type: "object",
    required: ["role"],
    properties: { role: { enum: ["editor", "viewer"] } },
    additionalProperties: false,
  },
  ArtifactMeta: {
    type: "object",
    required: ["id", "kind", "title", "ownerId", "size", "version", "starred", "createdAt", "updatedAt"],
    properties: {
      id: { type: "string" },
      kind: { enum: ["document", "spreadsheet", "presentation", "mindmap", "whiteboard"] },
      title: { type: "string" },
      ownerId: { type: "string" },
      size: { type: "integer", minimum: 0 },
      version: { type: "integer", minimum: 0 },
      starred: { type: "boolean" },
      createdAt: { type: "string", format: "date-time" },
      updatedAt: { type: "string", format: "date-time" },
    },
    additionalProperties: false,
  },
  ArtifactList: {
    type: "object",
    required: ["artifacts"],
    properties: { artifacts: { type: "array", items: ref("ArtifactMeta") } },
    additionalProperties: false,
  },
  SnapshotEnvelope: {
    type: "object",
    required: ["protocolVersion", "artifact"],
    properties: { protocolVersion: { type: "integer", minimum: 1 }, artifact: { type: "object" } },
    additionalProperties: false,
  },
  ProjectionEnvelope: {
    type: "object",
    required: ["protocolVersion", "contractVersion", "artifactId", "revision", "projection", "data", "truncated"],
    properties: {
      protocolVersion: { type: "integer", minimum: 1 },
      contractVersion: { type: "integer", minimum: 1 },
      artifactId: { type: "string" },
      revision: { type: "integer", minimum: 0 },
      projection: { enum: ["outline", "block", "presentation", "presentationOutline", "presentationSlide", "presentationNode"] },
      data: { oneOf: [ref("ProjectionList"), ref("BlockProjectionItem"), ref("PresentationDeckProjection"), ref("PresentationOutline"), ref("PresentationSlideProjection"), ref("PresentationNodeProjection")] },
      cursor: { type: "string" },
      nextCursor: { type: "string" },
      truncated: { type: "boolean" },
    },
    additionalProperties: false,
  },
  ProjectionList: {
    type: "object",
    required: ["items"],
    properties: { items: { type: "array", items: ref("ProjectionItem") } },
    additionalProperties: false,
  },
  ProjectionChild: {
    type: "object",
    required: ["blockId", "kind"],
    properties: {
      blockId: { type: "string", minLength: 1 },
      kind: { type: "string", minLength: 1 },
    },
    additionalProperties: false,
  },
  ProjectionItem: {
    type: "object",
    required: ["blockId", "kind", "parentId", "order", "children"],
    properties: {
      blockId: { type: "string", minLength: 1 },
      kind: { type: "string", minLength: 1 },
      parentId: { type: ["string", "null"] },
      order: { type: "integer", minimum: 0 },
      children: { type: "array", items: ref("ProjectionChild") },
      content: {},
      payload: {},
      refs: { type: "array", items: ref("CitationRef") },
      headingPath: { type: "array", items: { type: "string" } },
      sourceRef: {
        type: "object",
        required: ["artifactId", "blockId"],
        properties: {
          artifactId: { type: "string", minLength: 1 },
          blockId: { type: "string", minLength: 1 },
        },
        additionalProperties: false,
      },
    },
    additionalProperties: false,
  },
  CitationRef: {
    type: "object",
    required: ["artifactId", "blockId", "revision", "textRange", "headingPath"],
    properties: {
      artifactId: { type: "string", minLength: 1 },
      blockId: { type: "string", minLength: 1 },
      revision: { type: "integer", minimum: 0 },
      textRange: {
        type: "object",
        required: ["start", "end"],
        properties: {
          start: { type: "integer", minimum: 0 },
          end: { type: "integer", minimum: 0 },
        },
        additionalProperties: false,
      },
      headingPath: { type: "array", items: { type: "string" } },
      sourceUrl: { type: "string" },
    },
    additionalProperties: false,
  },
  BlockProjectionItem: {
    description: "A single block projection; payload and refs are optional and controlled by include.",
    "$ref": "#/components/schemas/ProjectionItem",
  },
  PresentationDeckProjection: {
    description: "Read-only v5 Presentation Deck summary; it never grants write capability.",
    type: "object",
    required: ["pageSpec", "themeId", "themeName", "slideCount", "masterCount", "layoutCount", "assetCount"],
    properties: {
      pageSpec: { type: "object" }, themeId: { type: "string" }, themeName: { type: "string" },
      slideCount: { type: "integer", minimum: 0 }, masterCount: { type: "integer", minimum: 0 },
      layoutCount: { type: "integer", minimum: 0 }, assetCount: { type: "integer", minimum: 0 },
    },
    additionalProperties: false,
  },
  PresentationOutline: {
    type: "object", required: ["items"],
    properties: { items: { type: "array", items: { type: "object", required: ["slideId", "orderKey", "name", "layoutId", "nodeCount", "timelineEntryCount", "hasNotes"], properties: { slideId: { type: "string" }, orderKey: { type: "string" }, name: { type: "string" }, layoutId: { type: ["string", "null"] }, nodeCount: { type: "integer", minimum: 0 }, timelineEntryCount: { type: "integer", minimum: 0 }, hasNotes: { type: "boolean" } }, additionalProperties: false } } },
    additionalProperties: false,
  },
  PresentationSlideProjection: {
    description: "Read-only slide metadata. nodes, assetIds, notes and timeline are opt-in via include.",
    type: "object", required: ["slideId", "orderKey", "name", "layoutId", "background"],
    properties: { slideId: { type: "string" }, orderKey: { type: "string" }, name: { type: "string" }, layoutId: { type: ["string", "null"] }, background: { type: "object" }, notes: { type: ["string", "null"] }, nodes: { type: "array", items: { type: "object" } }, assetIds: { type: "array", items: { type: "string" } }, timeline: { type: "object" } },
    additionalProperties: false,
  },
  PresentationNodeProjection: {
    description: "Read-only slide-scoped v5 node. assetIds and slideNodeIds permit strict SDK reference validation.",
    type: "object", required: ["slideId", "node", "childNodeIds", "assetIds", "slideNodeIds"],
    properties: { slideId: { type: "string" }, node: { type: "object" }, childNodeIds: { type: "array", items: { type: "string" } }, assetIds: { type: "array", items: { type: "string" } }, slideNodeIds: { type: "array", items: { type: "string" } } },
    additionalProperties: false,
  },
  EventPage: {
    type: "object",
    required: ["artifactId", "revision", "events"],
    properties: {
      artifactId: { type: "string" },
      revision: { type: "integer", minimum: 0 },
      events: { type: "array", items: { type: "object" } },
      nextCursor: { type: "string" },
    },
    additionalProperties: false,
  },
  ErrorEnvelope: {
    type: "object",
    required: ["error", "code", "requestId", "retryable"],
    properties: {
      error: { type: "string" },
      code: { type: "string" },
      requestId: { type: "string" },
      retryable: { type: "boolean" },
      details: { type: "object" },
    },
    additionalProperties: false,
  },
  CreateArtifactRequest: {
    type: "object",
    required: ["kind"],
    properties: {
      kind: { enum: ["document", "spreadsheet", "presentation", "mindmap", "whiteboard"] },
      title: { type: "string" },
    },
    additionalProperties: false,
  },
  PatchArtifactRequest: {
    type: "object",
    minProperties: 1,
    properties: { title: { type: "string" }, starred: { type: "boolean" } },
    additionalProperties: false,
  },
  ArtifactAsset: {
    type: "object",
    required: ["artifactId", "assetId", "objectKey", "contentType", "fileName", "checksum", "size", "refCount", "createdAt", "updatedAt"],
    properties: {
      artifactId: { type: "string", minLength: 1 },
      assetId: { type: "string", minLength: 1 },
      objectKey: { type: "string", minLength: 1 },
      contentType: { type: "string", minLength: 1 },
      fileName: { type: "string", minLength: 1 },
      checksum: { type: "string", pattern: "^[a-f0-9]{64}$" },
      size: { type: "integer", minimum: 1 },
      refCount: { type: "integer", minimum: 0 },
      createdAt: { type: "string", format: "date-time" },
      updatedAt: { type: "string", format: "date-time" },
    },
    additionalProperties: false,
  },
  AssetList: {
    type: "object",
    required: ["assets"],
    properties: { assets: { type: "array", items: ref("ArtifactAsset") } },
    additionalProperties: false,
  },
  DocumentReviewAnchor: {
    type: "object",
    required: ["blockId", "start", "end", "revision"],
    properties: {
      blockId: { type: "string", minLength: 1, maxLength: 128 },
      rowId: { type: "string", minLength: 1, maxLength: 128 },
      cellId: { type: "string", minLength: 1, maxLength: 128 },
      start: { type: "integer", minimum: 0 },
      end: { type: "integer", minimum: 0 },
      revision: { type: "integer", minimum: 0 },
    },
    additionalProperties: false,
  },
  DocumentReviewPage: {
    type: "object",
    required: ["artifactId", "revision", "threads"],
    properties: {
      artifactId: { type: "string" },
      revision: { type: "integer", minimum: 0 },
      threads: { type: "array", items: { type: "object" } },
    },
    additionalProperties: false,
  },
  CreateDocumentReview: {
    type: "object",
    required: ["threadId", "messageId", "anchor", "body", "mentions"],
    properties: {
      threadId: { type: "string", minLength: 1, maxLength: 128 },
      messageId: { type: "string", minLength: 1, maxLength: 128 },
      anchor: ref("DocumentReviewAnchor"),
      body: { type: "string", minLength: 1, maxLength: 10000 },
      mentions: { type: "array", maxItems: 32, uniqueItems: true, items: { type: "string" } },
    },
    additionalProperties: false,
  },
  CreateDocumentSuggestion: {
    type: "object",
    required: ["threadId", "messageId", "anchor", "body", "mentions", "suggestion"],
    properties: {
      threadId: { type: "string", minLength: 1, maxLength: 128 },
      messageId: { type: "string", minLength: 1, maxLength: 128 },
      anchor: ref("DocumentReviewAnchor"),
      body: { type: "string", minLength: 1, maxLength: 10000 },
      mentions: { type: "array", maxItems: 32, uniqueItems: true, items: { type: "string" } },
      suggestion: {
        type: "object",
        required: ["originalText", "replacement"],
        properties: { originalText: { type: "string" }, replacement: { type: "string", maxLength: 10000 } },
        additionalProperties: false,
      },
    },
    additionalProperties: false,
  },
  CreateDocumentReviewMessage: {
    type: "object",
    required: ["messageId", "body", "mentions"],
    properties: {
      messageId: { type: "string", minLength: 1, maxLength: 128 },
      body: { type: "string", minLength: 1, maxLength: 10000 },
      mentions: { type: "array", maxItems: 32, uniqueItems: true, items: { type: "string" } },
    },
    additionalProperties: false,
  },
  DocumentPresencePage: {
    type: "object",
    required: ["artifactId", "participants", "ttlMs"],
    properties: {
      artifactId: { type: "string" },
      participants: { type: "array", items: { type: "object" } },
      ttlMs: { type: "integer", minimum: 0 },
    },
    additionalProperties: false,
  },
  DocumentPresenceUpdate: {
    type: "object",
    required: ["revision", "selectedNodeIds"],
    properties: {
      revision: { type: "integer", minimum: 0 },
      blockId: { type: "string", minLength: 1, maxLength: 128 },
      selectedNodeIds: { type: "array", maxItems: 0 },
      selection: { type: "object" },
    },
    additionalProperties: false,
  },
};

const jsonBody = (schema, description = "JSON request") => ({
  required: true,
  description,
  ...json({ "$ref": `#/components/schemas/${schema}` }),
});

export function buildOpenApi() {
  const paths = {
    "/api/health": { get: { operationId: "health", responses: { "200": { description: "Healthy" } } } },
    "/api/capabilities": { get: { operationId: "getCapabilities", responses: { "200": json(ref("CapabilityCatalog")) } } },
    "/api/artifacts": {
      get: { operationId: "listArtifacts", responses: { "200": json(ref("ArtifactList")) } },
      post: {
        operationId: "createArtifact",
        requestBody: jsonBody("CreateArtifactRequest"),
        responses: { "201": artifactResponse, "400": errorResponse },
      },
    },
    "/api/artifacts/import": {
      post: { operationId: "importArtifact", requestBody: { required: true, content: { "multipart/form-data": { schema: { type: "object", required: ["file"], properties: { file: { type: "string", format: "binary" }, mode: { type: "string", enum: ["audit", "strict"], default: "audit" } } } } } }, responses: { "201": artifactResponse, "400": errorResponse, "422": errorResponse, "501": errorResponse } },
    },
    "/api/artifacts/{id}": {
      parameters: [artifactId],
      get: { operationId: "getArtifactMeta", responses: { "200": artifactResponse, "404": errorResponse } },
      patch: { operationId: "patchArtifact", requestBody: jsonBody("PatchArtifactRequest"), responses: { "200": artifactResponse, "404": errorResponse } },
      delete: { operationId: "deleteArtifact", responses: { "204": { description: "Deleted" }, "404": errorResponse } },
    },
    "/api/artifacts/{id}/snapshot": {
      parameters: [artifactId],
      get: { operationId: "getSnapshot", responses: { "200": json(ref("SnapshotEnvelope")), "404": errorResponse } },
    },
    "/api/artifacts/{id}/transactions": {
      parameters: [artifactId],
      post: { operationId: "submitTransaction", parameters: [ifMatch, transactionId], requestBody: jsonBody("ArtifactCommandEnvelope"), responses: { "200": commitResponse, "409": errorResponse, "501": errorResponse } },
    },
    "/api/artifacts/{id}/outline": {
      parameters: [artifactId],
      get: { operationId: "getOutline", parameters: [{ "$ref": "#/components/parameters/ProjectionInclude" }, { "$ref": "#/components/parameters/ProjectionCursor" }, { "$ref": "#/components/parameters/ProjectionMaxBytes" }], responses: { "200": json(ref("ProjectionEnvelope")), "413": errorResponse } },
    },
    "/api/artifacts/{id}/toc": {
      parameters: [artifactId],
      get: { operationId: "getTableOfContents", parameters: [{ "$ref": "#/components/parameters/ProjectionCursor" }, { "$ref": "#/components/parameters/ProjectionMaxBytes" }, { name: "limit", in: "query", schema: { type: "integer", minimum: 1, maximum: 1000 } }], responses: { "200": json(ref("ProjectionEnvelope")), "400": errorResponse, "413": errorResponse } },
    },
    "/api/artifacts/{id}/projection/documentPrint": {
      parameters: [artifactId],
      get: { operationId: "getDocumentPrintProjection", responses: { "200": json(ref("ProjectionEnvelope")), "304": { description: "ETag matched current revision" }, "404": errorResponse } },
    },
    "/api/artifacts/{id}/blocks": {
      parameters: [artifactId],
      get: { operationId: "listBlocks", parameters: [{ "$ref": "#/components/parameters/ProjectionInclude" }, { "$ref": "#/components/parameters/ProjectionCursor" }, { "$ref": "#/components/parameters/ProjectionMaxBytes" }, { name: "parentId", in: "query", schema: { type: "string" } }, { name: "limit", in: "query", schema: { type: "integer", minimum: 1, maximum: 1000 } }], responses: { "200": json(ref("ProjectionEnvelope")), "413": errorResponse } },
    },
    "/api/artifacts/{id}/blocks/{blockId}": {
      parameters: [artifactId, blockId],
      get: { operationId: "getBlock", parameters: [{ "$ref": "#/components/parameters/ProjectionInclude" }, { "$ref": "#/components/parameters/ProjectionCursor" }, { "$ref": "#/components/parameters/ProjectionMaxBytes" }], responses: { "200": json(ref("ProjectionEnvelope")), "404": errorResponse, "413": errorResponse } },
    },
    "/api/artifacts/{id}/projection/spreadsheet": {
      parameters: [artifactId, { name: "sheetId", in: "query", required: true, schema: { type: "string", minLength: 1 } },
        { name: "startRow", in: "query", required: true, schema: { type: "integer", minimum: 0 } },
        { name: "endRow", in: "query", required: true, schema: { type: "integer", minimum: 1 } },
        { name: "startColumn", in: "query", required: true, schema: { type: "integer", minimum: 0 } },
        { name: "endColumn", in: "query", required: true, schema: { type: "integer", minimum: 1 } }],
      get: { operationId: "getSpreadsheetGrid", responses: { "200": json(ref("ProjectionEnvelope")), "304": { description: "ETag matched current revision" }, "400": errorResponse, "403": errorResponse, "413": errorResponse } },
    },
    "/api/artifacts/{id}/projection/presentation": {
      parameters: [artifactId],
      get: { operationId: "getPresentationDeckProjection", responses: { "200": json(ref("ProjectionEnvelope")), "304": { description: "ETag matched current revision" }, "404": errorResponse } },
    },
    "/api/artifacts/{id}/presentation/outline": {
      parameters: [artifactId],
      get: { operationId: "getPresentationOutline", parameters: [{ "$ref": "#/components/parameters/ProjectionCursor" }, { "$ref": "#/components/parameters/ProjectionMaxBytes" }, { name: "limit", in: "query", schema: { type: "integer", minimum: 1, maximum: 1000 } }], responses: { "200": json(ref("ProjectionEnvelope")), "304": { description: "ETag matched current revision" }, "400": errorResponse, "413": errorResponse } },
    },
    "/api/artifacts/{id}/presentation/slides/{slideId}": {
      parameters: [artifactId, { name: "slideId", in: "path", required: true, schema: { type: "string", minLength: 1 } }],
      get: { operationId: "getPresentationSlide", parameters: [{ "$ref": "#/components/parameters/PresentationSlideInclude" }, { "$ref": "#/components/parameters/ProjectionMaxBytes" }], responses: { "200": json(ref("ProjectionEnvelope")), "304": { description: "ETag matched current revision" }, "404": errorResponse, "413": errorResponse } },
    },
    "/api/artifacts/{id}/presentation/slides/{slideId}/nodes/{nodeId}": {
      parameters: [artifactId, { name: "slideId", in: "path", required: true, schema: { type: "string", minLength: 1 } }, { name: "nodeId", in: "path", required: true, schema: { type: "string", minLength: 1 } }],
      get: { operationId: "getPresentationNode", parameters: [{ "$ref": "#/components/parameters/ProjectionMaxBytes" }], responses: { "200": json(ref("ProjectionEnvelope")), "304": { description: "ETag matched current revision" }, "404": errorResponse, "413": errorResponse } },
    },
    "/api/artifacts/{id}/assets": {
      parameters: [artifactId],
      get: { operationId: "listAssets", responses: { "200": json(ref("AssetList")), "404": errorResponse } },
      post: {
        operationId: "uploadAsset",
        requestBody: {
          required: true,
          content: {
            "multipart/form-data": {
              schema: {
                type: "object",
                required: ["file"],
                properties: { file: { type: "string", format: "binary" } },
                additionalProperties: false,
              },
            },
          },
        },
        responses: { "201": json(ref("ArtifactAsset")), "400": errorResponse, "404": errorResponse, "413": errorResponse },
      },
    },
    "/api/artifacts/{id}/assets/{assetId}": {
      parameters: [artifactId, { name: "assetId", in: "path", required: true, schema: { type: "string", minLength: 1 } }],
      get: { operationId: "getAsset", responses: { "200": { description: "Verified asset bytes" }, "404": errorResponse, "409": errorResponse } },
      delete: { operationId: "deleteAsset", responses: { "204": { description: "Deleted" }, "400": errorResponse, "404": errorResponse } },
    },
    "/api/artifacts/{id}/events": {
      parameters: [artifactId],
      get: { operationId: "listEvents", parameters: [{ name: "sinceRevision", in: "query", schema: { type: "integer", minimum: 0 } }, { name: "cursor", in: "query", schema: { type: "string" } }, { name: "limit", in: "query", schema: { type: "integer", minimum: 1, maximum: 1000 } }], responses: { "200": json(ref("EventPage")), "400": errorResponse } },
    },
    "/api/artifacts/{id}/event-stream": {
      parameters: [artifactId],
      get: { operationId: "streamArtifactEvents", parameters: [{ name: "sinceRevision", in: "query", description: "Exclusive durable revision cursor used for reconnect.", schema: { type: "integer", minimum: 0 } }], responses: { "200": { description: "Server-sent durable revision notices and ephemeral presence snapshots", content: { "text/event-stream": { schema: { type: "string" } } } }, "404": errorResponse } },
    },
    "/api/artifacts/{id}/reviews": {
      parameters: [artifactId],
      get: { operationId: "listDocumentReviews", responses: { "200": json(ref("DocumentReviewPage")), "404": errorResponse } },
      post: { operationId: "createDocumentReview", requestBody: jsonBody("CreateDocumentReview"), responses: { "201": json(ref("DocumentReviewPage")), "400": errorResponse, "409": errorResponse } },
    },
    "/api/artifacts/{id}/suggestions": {
      parameters: [artifactId],
      post: { operationId: "createDocumentSuggestion", requestBody: jsonBody("CreateDocumentSuggestion"), responses: { "201": json(ref("DocumentReviewPage")), "400": errorResponse, "409": errorResponse } },
    },
    "/api/artifacts/{id}/reviews/{threadId}": {
      parameters: [artifactId, { name: "threadId", in: "path", required: true, schema: { type: "string", minLength: 1, maxLength: 128 } }],
      patch: { operationId: "updateDocumentReview", requestBody: { required: true, ...json({ type: "object", required: ["state"], properties: { state: { enum: ["open", "resolved", "accepted", "rejected"] } }, additionalProperties: false }) }, responses: { "204": { description: "Review state updated" }, "400": errorResponse, "404": errorResponse } },
    },
    "/api/artifacts/{id}/reviews/{threadId}/messages": {
      parameters: [artifactId, { name: "threadId", in: "path", required: true, schema: { type: "string", minLength: 1, maxLength: 128 } }],
      post: { operationId: "replyDocumentReview", requestBody: jsonBody("CreateDocumentReviewMessage"), responses: { "201": { description: "Reply created" }, "400": errorResponse, "404": errorResponse } },
    },
    "/api/artifacts/{id}/presence": {
      parameters: [artifactId],
      get: { operationId: "getArtifactPresence", responses: { "200": json(ref("DocumentPresencePage")), "404": errorResponse } },
    },
    "/api/artifacts/{id}/presence/{sessionId}": {
      parameters: [artifactId, { name: "sessionId", in: "path", required: true, schema: { type: "string", minLength: 1, maxLength: 128 } }],
      put: { operationId: "updateArtifactPresence", requestBody: jsonBody("DocumentPresenceUpdate"), responses: { "204": { description: "Ephemeral presence refreshed" }, "400": errorResponse, "409": errorResponse } },
    },
    "/api/artifacts/{id}/revisions": { parameters: [artifactId], get: { operationId: "listRevisions", responses: { "200": { description: "Revision metadata", content: { "application/json": { schema: { type: "array", items: { type: "object" } } } } } } } },
    "/api/artifacts/{id}/revisions/{version}": { parameters: [artifactId, version], get: { operationId: "getRevision", responses: { "200": json(ref("SnapshotEnvelope")), "404": errorResponse } } },
    "/api/artifacts/{id}/revisions/{version}/restore": { parameters: [artifactId, version], post: { operationId: "restoreRevision", parameters: [ifMatch, transactionId], responses: { "200": commitResponse, "409": errorResponse } } },
    "/api/artifacts/{id}/history": { parameters: [artifactId], get: { operationId: "getHistory", responses: { "200": { description: "History state", content: { "application/json": { schema: { type: "object", required: ["canUndo", "canRedo"], properties: { canUndo: { type: "boolean" }, canRedo: { type: "boolean" } }, additionalProperties: false } } } } } } },
    "/api/artifacts/{id}/collaborators": { parameters: [artifactId], get: { operationId: "listCollaborators", responses: { "200": json(ref("CollaboratorList")), "403": errorResponse } } },
    "/api/artifacts/{id}/collaborators/{userId}": {
      parameters: [artifactId, { name: "userId", in: "path", required: true, schema: { type: "string", minLength: 1, maxLength: 128 } }],
      put: { operationId: "upsertCollaborator", requestBody: json(ref("UpsertCollaboratorRequest")), responses: { "204": { description: "Role granted or updated" }, "400": errorResponse, "403": errorResponse } },
      delete: { operationId: "deleteCollaborator", responses: { "204": { description: "Collaborator removed" }, "403": errorResponse } },
    },
    "/api/artifacts/{id}/source": { parameters: [artifactId], get: { operationId: "getOriginalSource", responses: { "200": { description: "Original source bytes" }, "404": errorResponse } } },
    "/api/artifacts/{id}/export/{format}": { parameters: [artifactId, { name: "format", in: "path", required: true, schema: { type: "string", enum: ["docx", "xlsx", "pptx", "json", "md", "svg", "pdf"] } }], get: { operationId: "exportArtifact", parameters: [{ name: "paper", in: "query", schema: { type: "string", enum: ["a4", "a3"] } }, { name: "orientation", in: "query", schema: { type: "string", enum: ["portrait", "landscape"] } }, { name: "mode", in: "query", schema: { type: "string", enum: ["fit", "tile"] } }, { name: "margin", in: "query", schema: { type: "number", minimum: 0 } }], responses: { "200": { description: "Exported artifact bytes" }, "400": errorResponse } } },
  };
  return {
    openapi: "3.1.0",
    info: { title: "open-office Artifact API", version: "0.1.0", description: "Canonical REST boundary for all Artifact clients." },
    servers: [{ url: "http://127.0.0.1:8787" }],
    paths,
    components: {
      parameters: {
        ProjectionInclude: {
          name: "include",
          in: "query",
          description: "Comma-separated projection includes: content, headingPath, refs (single-block only).",
          schema: { type: "string" },
        },
        ProjectionCursor: { name: "cursor", in: "query", description: "Opaque cursor tied to the snapshot revision.", schema: { type: "string" } },
        ProjectionMaxBytes: { name: "maxBytes", in: "query", description: "Response byte budget (256..4194304).", schema: { type: "integer", minimum: 256, maximum: 4194304 } },
        PresentationSlideInclude: { name: "include", in: "query", description: "Comma-separated slide sections: nodes, notes, timeline. Defaults to metadata only.", schema: { type: "string" } },
      },
      schemas: {
        ...protocolSchemas,
        ...schemas,
      },
    },
  };
}

// ---------------------------------------------------------------------------
// MCP tool manifest.
//
// The tool surface is curated: tool names, agent-facing rules and query hints
// are editorial, and there is no way to derive them from an HTTP schema. What
// *is* mechanical — the method, the path, the result schema and the required
// headers — is read back out of `buildOpenApi()` and cross-checked against the
// declaration below. A renamed path or a changed response type therefore fails
// the drift check instead of silently leaving a stale manifest behind.
// ---------------------------------------------------------------------------

const MCP_MECHANICAL_KEYS = new Set([
  "name",
  "method",
  "path",
  "resultSchema",
  "requestSchema",
  "query",
  "headers",
  "readOnly",
]);

const MCP_TOOLS = [
  {
    name: "artifact_capabilities",
    method: "GET",
    path: "/api/capabilities",
    resultSchema: "CapabilityCatalog",
  },
  {
    name: "artifact_outline",
    method: "GET",
    path: "/api/artifacts/{id}/outline",
    resultSchema: "ProjectionEnvelope",
    query: ["cursor", "limit", "maxBytes", "include"],
  },
  {
    name: "artifact_blocks",
    method: "GET",
    path: "/api/artifacts/{id}/blocks",
    resultSchema: "ProjectionEnvelope",
    query: ["parentId", "cursor", "limit", "maxBytes", "include=content,headingPath,refs"],
    citationRule:
      "When refs is requested, persist sourceRef/citation fields with artifactId, blockId and revision.",
  },
  {
    name: "artifact_block",
    method: "GET",
    path: "/api/artifacts/{id}/blocks/{blockId}",
    resultSchema: "ProjectionEnvelope",
    query: ["cursor", "maxBytes", "include=content,headingPath,refs"],
  },
  {
    name: "presentation_deck",
    method: "GET",
    path: "/api/artifacts/{id}/projection/presentation",
    resultSchema: "ProjectionEnvelope",
    capabilityRule:
      "This tool observes canonical Deck facts. Presentation writes require a fresh capability lookup and one of the advertised typed commands.",
  },
  {
    name: "presentation_outline",
    method: "GET",
    path: "/api/artifacts/{id}/presentation/outline",
    resultSchema: "ProjectionEnvelope",
    query: ["cursor", "limit", "maxBytes"],
    cursorRule:
      "Cursor is opaque and revision-bound; on expiration restart from the latest outline.",
  },
  {
    name: "presentation_slide",
    method: "GET",
    path: "/api/artifacts/{id}/presentation/slides/{slideId}",
    resultSchema: "ProjectionEnvelope",
    query: ["include=nodes,notes,timeline", "maxBytes"],
    includeRule:
      "The default is metadata-only. Request large sections explicitly and preserve the response revision.",
  },
  {
    name: "presentation_node",
    method: "GET",
    path: "/api/artifacts/{id}/presentation/slides/{slideId}/nodes/{nodeId}",
    resultSchema: "ProjectionEnvelope",
    query: ["maxBytes"],
    identityRule:
      "A node is addressed by (slideId,nodeId); never resolve a bare nodeId across slides.",
  },
  {
    name: "artifact_events",
    method: "GET",
    path: "/api/artifacts/{id}/events",
    resultSchema: "EventPage",
    query: ["sinceRevision", "cursor", "limit"],
    delivery: "at-least-once",
    dedupeKey: "eventId",
    gapRule: "A consumer that observes a revision gap must re-read a projection or snapshot.",
  },
  {
    name: "artifact_transaction",
    method: "POST",
    path: "/api/artifacts/{id}/transactions",
    resultSchema: "CommitResult",
    requestSchema: "ArtifactCommandEnvelope",
    conflictRule:
      "409 version_conflict is re-read/rebase; reusing the same transactionId is idempotent.",
  },
];

/** The agent-facing manifest spells the artifact path parameter out in full. */
function mcpPath(path) {
  return path.replace("{id}", "{artifactId}");
}

function mcpResultSchema(operation) {
  const response = operation.responses?.["200"] ?? operation.responses?.["204"];
  const schema = response?.content?.["application/json"]?.schema;
  return schema?.$ref ? schema.$ref.replace("#/components/schemas/", "") : undefined;
}

export function buildMcpTools() {
  const { paths } = buildOpenApi();
  return {
    manifestVersion: 1,
    contract: "scripts/generated/openapi.json",
    capabilitiesEndpoint: "GET /api/capabilities",
    transport: {
      baseUrl: "http://127.0.0.1:8787",
      revisionHeader: "If-Match",
      idempotencyHeader: "x-transaction-id",
      revisionRule: "If-Match must equal envelope.baseRevision",
      transactionRule: "x-transaction-id must equal envelope.transactionId",
    },
    tools: MCP_TOOLS.map((tool) => {
      const item = paths[tool.path];
      if (!item) {
        throw new Error(`mcp tool ${tool.name}: ${tool.path} is not in the OpenAPI contract`);
      }
      const operation = item[tool.method.toLowerCase()];
      if (!operation) {
        throw new Error(
          `mcp tool ${tool.name}: ${tool.method} ${tool.path} is not in the OpenAPI contract`,
        );
      }
      const declared = mcpResultSchema(operation);
      if (declared && tool.resultSchema !== declared) {
        throw new Error(
          `mcp tool ${tool.name}: declares resultSchema ${tool.resultSchema} but ` +
            `${tool.method} ${tool.path} returns ${declared}`,
        );
      }
      const entry = {
        name: tool.name,
        method: tool.method,
        path: mcpPath(tool.path),
        readOnly: tool.method === "GET",
      };
      if (tool.query) entry.query = tool.query;
      const headers = (operation.parameters ?? [])
        .filter((parameter) => parameter.in === "header")
        .map((parameter) => parameter.name);
      if (headers.length > 0) entry.headers = headers;
      if (tool.requestSchema) entry.requestSchema = tool.requestSchema;
      entry.resultSchema = tool.resultSchema;
      for (const [key, value] of Object.entries(tool)) {
        if (!MCP_MECHANICAL_KEYS.has(key)) entry[key] = value;
      }
      return entry;
    }),
  };
}

export function generatedFiles() {
  const openapi = buildOpenApi();
  return {
    "scripts/generated/openapi.json": openapi,
    // The agent-facing tool manifest is derived from the same contract it
    // describes, so it cannot advertise a route or a result type that the API
    // no longer serves.
    "scripts/generated/mcp-tools.json": buildMcpTools(),
    // Server DTOs and generated protocol types each get one stable file.
    ...Object.fromEntries(
      Object.entries({ ...protocolSchemas, ...schemas }).map(([name, schema]) => [
        `scripts/generated/api-schemas/${name}.json`,
        schema,
      ]),
    ),
  };
}
