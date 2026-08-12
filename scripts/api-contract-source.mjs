/**
 * The HTTP contract has one executable source.  Rust owns runtime validation;
 * this source describes the public transport surface and is intentionally
 * limited to DTO shape, methods and headers so generated docs cannot invent a
 * second domain model.
 */

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

export const schemas = {
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
  CapabilityCatalog: {
    type: "object",
    required: ["protocolVersion", "contractVersion", "transport", "artifacts"],
    properties: {
      protocolVersion: { type: "integer", minimum: 1 },
      contractVersion: { type: "integer", minimum: 1 },
      transport: {
        type: "object",
        required: ["snapshotEndpoint", "transactionEndpoint", "revisionHeader", "idempotencyHeader"],
        properties: {
          snapshotEndpoint: { type: "string" },
          transactionEndpoint: { type: "string" },
          revisionHeader: { type: "string" },
          idempotencyHeader: { type: "string" },
        },
        additionalProperties: false,
      },
      artifacts: {
        type: "array",
        items: {
          type: "object",
          required: ["kind", "namespace", "status", "commands"],
          properties: {
            kind: { enum: ["document", "spreadsheet", "presentation", "mindmap", "whiteboard"] },
            namespace: { type: "string" },
            status: { enum: ["stable", "planned"] },
            commands: {
              type: "array",
              items: {
                type: "object",
                required: ["typeId", "scope", "requiresRevision", "supportsIdempotency"],
                properties: {
                  typeId: { type: "string" },
                  scope: { type: "string" },
                  requiresRevision: { type: "boolean" },
                  supportsIdempotency: { type: "boolean" },
                },
                additionalProperties: false,
              },
            },
          },
          additionalProperties: false,
        },
      },
    },
    additionalProperties: false,
  },
  ArtifactCommandEnvelope: {
    type: "object",
    required: ["protocolVersion", "transactionId", "intentId", "artifactId", "actorId", "baseRevision", "origin", "commands"],
    properties: {
      protocolVersion: { type: "integer", minimum: 1 },
      transactionId: { type: "string", minLength: 1 },
      intentId: { type: "string", minLength: 1 },
      artifactId: { type: "string", minLength: 1 },
      actorId: { type: "string", minLength: 1 },
      baseRevision: { type: "integer", minimum: 0 },
      origin: { type: "string" },
      commands: { type: "array", minItems: 1, items: { type: "object" } },
    },
    additionalProperties: false,
  },
  SnapshotEnvelope: {
    type: "object",
    required: ["protocolVersion", "artifact"],
    properties: { protocolVersion: { type: "integer", minimum: 1 }, artifact: { type: "object" } },
    additionalProperties: false,
  },
  CommitResult: {
    type: "object",
    required: ["protocolVersion", "artifactId", "transactionId", "baseRevision", "revision", "invalidation", "mutations", "events"],
    properties: {
      protocolVersion: { type: "integer", minimum: 1 },
      artifactId: { type: "string" },
      transactionId: { type: "string" },
      baseRevision: { type: "integer", minimum: 0 },
      revision: { type: "integer", minimum: 0 },
      invalidation: { type: "object" },
      mutations: { type: "array", items: { type: "object" } },
      events: { type: "array", items: { type: "object" } },
    },
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
    required: ["error", "code", "requestId"],
    properties: {
      error: { type: "string" },
      code: { type: "string" },
      requestId: { type: "string" },
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
      post: { operationId: "importArtifact", requestBody: { required: true, content: { "multipart/form-data": { schema: { type: "object", required: ["file"], properties: { file: { type: "string", format: "binary" } } } } } }, responses: { "201": artifactResponse, "422": errorResponse } },
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
      put: { operationId: "putSnapshot", parameters: [ifMatch, transactionId], requestBody: jsonBody("SnapshotEnvelope"), responses: { "200": commitResponse, "409": errorResponse } },
    },
    "/api/artifacts/{id}/transactions": {
      parameters: [artifactId],
      post: { operationId: "submitTransaction", parameters: [ifMatch, transactionId], requestBody: jsonBody("ArtifactCommandEnvelope"), responses: { "200": commitResponse, "409": errorResponse, "501": errorResponse } },
    },
    "/api/artifacts/{id}/outline": {
      parameters: [artifactId],
      get: { operationId: "getOutline", parameters: [{ "$ref": "#/components/parameters/ProjectionInclude" }, { "$ref": "#/components/parameters/ProjectionCursor" }, { "$ref": "#/components/parameters/ProjectionMaxBytes" }], responses: { "200": json(ref("ProjectionEnvelope")), "413": errorResponse } },
    },
    "/api/artifacts/{id}/blocks": {
      parameters: [artifactId],
      get: { operationId: "listBlocks", parameters: [{ "$ref": "#/components/parameters/ProjectionInclude" }, { "$ref": "#/components/parameters/ProjectionCursor" }, { "$ref": "#/components/parameters/ProjectionMaxBytes" }, { name: "parentId", in: "query", schema: { type: "string" } }, { name: "limit", in: "query", schema: { type: "integer", minimum: 1, maximum: 1000 } }], responses: { "200": json(ref("ProjectionEnvelope")), "413": errorResponse } },
    },
    "/api/artifacts/{id}/blocks/{blockId}": {
      parameters: [artifactId, blockId],
      get: { operationId: "getBlock", parameters: [{ "$ref": "#/components/parameters/ProjectionInclude" }, { "$ref": "#/components/parameters/ProjectionCursor" }, { "$ref": "#/components/parameters/ProjectionMaxBytes" }], responses: { "200": json(ref("ProjectionEnvelope")), "404": errorResponse, "413": errorResponse } },
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
    "/api/artifacts/{id}/revisions": { parameters: [artifactId], get: { operationId: "listRevisions", responses: { "200": { description: "Revision metadata", content: { "application/json": { schema: { type: "array", items: { type: "object" } } } } } } } },
    "/api/artifacts/{id}/revisions/{version}": { parameters: [artifactId, version], get: { operationId: "getRevision", responses: { "200": json(ref("SnapshotEnvelope")), "404": errorResponse } } },
    "/api/artifacts/{id}/revisions/{version}/restore": { parameters: [artifactId, version], post: { operationId: "restoreRevision", parameters: [ifMatch, transactionId], responses: { "200": commitResponse, "409": errorResponse } } },
    "/api/artifacts/{id}/history": { parameters: [artifactId], get: { operationId: "getHistory", responses: { "200": { description: "History state", content: { "application/json": { schema: { type: "object", required: ["canUndo", "canRedo"], properties: { canUndo: { type: "boolean" }, canRedo: { type: "boolean" } }, additionalProperties: false } } } } } } },
    "/api/artifacts/{id}/source": { parameters: [artifactId], get: { operationId: "getOriginalSource", responses: { "200": { description: "Original source bytes" }, "404": errorResponse } } },
    "/api/artifacts/{id}/export/{format}": { parameters: [artifactId, { name: "format", in: "path", required: true, schema: { type: "string" } }], get: { operationId: "exportArtifact", responses: { "200": { description: "Exported artifact bytes" }, "400": errorResponse } } },
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
        ...schemas,
      },
    },
  };
}

export function generatedFiles() {
  const openapi = buildOpenApi();
  return {
    "docs/generated/openapi.json": openapi,
    ...Object.fromEntries(Object.entries(schemas).map(([name, schema]) => [`docs/generated/api-schemas/${name}.json`, schema])),
  };
}
