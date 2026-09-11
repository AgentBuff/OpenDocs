#!/usr/bin/env node

/**
 * Runtime architecture gate.
 *
 * This is intentionally a small dependency-free source check. It only scans active runtime
 * directories; ADRs, findings, fixtures, tests, generated wasm and the frozen history are not
 * executable architecture and therefore are excluded. The gate prevents the old browser
 * Document engine from silently returning through a new feature branch.
 */

import { readFile, readdir } from "node:fs/promises";
import { join, relative, resolve } from "node:path";

const webRoot = resolve(new URL("..", import.meta.url).pathname);
const root = resolve(webRoot, "..");
const scanRoots = [
  resolve(webRoot, "apps/editor/src"),
  resolve(webRoot, "packages/schema/src"),
  resolve(webRoot, "packages/document-engine/src"),
  resolve(webRoot, "packages/mindmap-engine/src"),
  resolve(root, "crates/oo-document/src"),
  resolve(root, "crates/oo-document-wasm/src"),
  resolve(root, "crates/oo-mindmap-wasm/src"),
  resolve(root, "crates/oo-protocol/src"),
  resolve(root, "crates/oo-server/src"),
  // Every non-document Artifact is a first-class runtime as well. Keep their command
  // boundaries explicit so an old Transaction/Operation API cannot return unnoticed.
  resolve(root, "crates/oo-spreadsheet/src"),
  resolve(root, "crates/oo-presentation/src"),
  resolve(root, "crates/oo-mindmap/src"),
  resolve(root, "crates/oo-whiteboard/src"),
  resolve(root, "crates/oo-xlsx/src"),
  resolve(root, "crates/oo-pptx/src"),
];

const forbidden = [
  { name: "legacy DocumentOperation type", pattern: /\bDocumentOperation\b/g },
  { name: "legacy DocumentTransaction engine type", pattern: /\bDocumentTransaction\b/g },
  { name: "frontend applyOperation helper", pattern: /\bapplyOperation\b/g },
  { name: "frontend invertOperations helper", pattern: /\binvertOperations\b/g },
  { name: "removed operation journal column", pattern: /\boperations_json\b/g },
  // Match endpoint construction, not OOXML/XLSX ContentType XML strings in format adapters.
  { name: "removed legacy content endpoint", pattern: /(?:route|fetch|axios|api|url|endpoint)[^\n]*\/content\b/gi },
  { name: "removed legacy operations endpoint", pattern: /(?:route|fetch|axios|api|url|endpoint)[^\n]*\/operations\b/gi },
  { name: "full-model clone in runtime", pattern: /\bstructuredClone\s*\(/g },
  { name: "legacy link URL stored in attrs", pattern: /\battrs\s*\.\s*url\b/g },
  { name: "legacy link URL map lookup", pattern: /\battrs\s*\[\s*["']url["']\s*\]/g },
  { name: "generic block payload writer", pattern: /\bupdateBlockPayload\b/g },
  { name: "generic block-kind writer", pattern: /\bupdateKind\b/g },
  { name: "retired generic block command", pattern: /\bDocumentCommand::UpdateBlock\b/g },
  { name: "retired generic block type id", pattern: /\bdocument\.updateBlock\b/g },
  { name: "retired generic block wire tag", pattern: /\btype\s*:\s*["']updateBlock["']/g },
  { name: "old engine apply entry point", pattern: /DocumentEngine::apply\b/g },
  { name: "legacy SpreadsheetTransaction type", pattern: /\bSpreadsheetTransaction\b/g },
  { name: "legacy SpreadsheetOperation type", pattern: /\bSpreadsheetOperation\b/g },
  { name: "legacy SpreadsheetTransactionResult type", pattern: /\bSpreadsheetTransactionResult\b/g },
  { name: "legacy PresentationTransaction type", pattern: /\bPresentationTransaction\b/g },
  { name: "legacy PresentationOperation type", pattern: /\bPresentationOperation\b/g },
  { name: "legacy PresentationTransactionResult type", pattern: /\bPresentationTransactionResult\b/g },
  { name: "legacy MindmapTransaction type", pattern: /\bMindmapTransaction\b/g },
  { name: "legacy MindmapOperation type", pattern: /\bMindmapOperation\b/g },
  { name: "legacy MindmapTransactionResult type", pattern: /\bMindmapTransactionResult\b/g },
  { name: "legacy WhiteboardTransaction type", pattern: /\bWhiteboardTransaction\b/g },
  { name: "legacy WhiteboardOperation type", pattern: /\bWhiteboardOperation\b/g },
  { name: "legacy WhiteboardTransactionResult type", pattern: /\bWhiteboardTransactionResult\b/g },
];

const files = [];
for (const scanRoot of scanRoots) await collect(scanRoot, files);

// I01-09: block renderer/chrome modules must not own global event lifecycles.
// Escape/outside-click arbitration belongs to the interaction OverlayCoordinator,
// and keyboard routing belongs to keyboardRouter. The only approved exceptions
// are transient pointer-drag capture and measurement adapters reviewed under
// I01-07/I01-08; a new listener needs an explicit allowlist entry plus rationale.
const editorSrcRoot = resolve(webRoot, "apps/editor/src");
const rendererListenerApproved = new Set([
  "web/apps/editor/src/blocks/table/useTableResize.ts",
  "web/apps/editor/src/blocks/table/useTableGeometry.ts",
  "web/apps/editor/src/blocks/table/useTableSelectionController.ts",
  "web/apps/editor/src/blocks/code/CodeBlockView.tsx",
]);
const rendererListenerRoots = [
  resolve(editorSrcRoot, "blocks"),
  resolve(editorSrcRoot, "chrome"),
];
const rendererListenerFiles = [];
for (const scanRoot of rendererListenerRoots) await collect(scanRoot, rendererListenerFiles);

const violations = [];
for (const file of files) {
  const source = await readFile(file, "utf8");
  for (const rule of forbidden) {
    rule.pattern.lastIndex = 0;
    let match;
    while ((match = rule.pattern.exec(source)) !== null) {
      const line = source.slice(0, match.index).split("\n").length;
      violations.push(`${relative(root, file)}:${line} ${rule.name}`);
    }
  }
}

for (const file of rendererListenerFiles) {
  const relativePath = relative(root, file);
  if (rendererListenerApproved.has(relativePath.replaceAll("\\", "/"))) continue;
  const source = await readFile(file, "utf8");
  const listenerPattern = /\b(?:window|document)\.addEventListener\s*\(/g;
  let match;
  while ((match = listenerPattern.exec(source)) !== null) {
    const line = source.slice(0, match.index).split("\n").length;
    violations.push(
      `${relativePath}:${line} global event listener in renderer module (approved drag/measurement adapters only; extend the allowlist with rationale)`,
    );
  }
}

if (violations.length > 0) {
  console.error("Architecture gate failed:");
  for (const violation of violations) console.error(`- ${violation}`);
  process.exitCode = 1;
} else {
  console.log(`Architecture gate passed (${files.length} active source files scanned).`);
}

async function collect(path, output) {
  let entries;
  try {
    entries = await readdir(path, { withFileTypes: true });
  } catch (error) {
    if (error?.code === "ENOENT") return;
    throw error;
  }
  for (const entry of entries) {
    if (entry.name === "node_modules" || entry.name === "wasm") continue;
    const entryPath = join(path, entry.name);
    if (entry.isDirectory()) {
      await collect(entryPath, output);
    } else if (/\.(?:ts|tsx|rs)$/.test(entry.name)) {
      output.push(entryPath);
    }
  }
}
