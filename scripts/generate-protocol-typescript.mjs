#!/usr/bin/env node

/**
 * TypeScript protocol type generation (ADR-0010 phase 3).
 *
 * Source of truth is the Rust golden snapshot produced by
 * `oo_protocol::generate_contract_schemas()` and locked by a cargo test.
 * This script flattens it (same normalization as the OpenAPI generator) and
 * emits `web/packages/schema/src/protocol.generated.ts`.
 *
 * Usage:
 *   node scripts/generate-protocol-typescript.mjs          # write the file
 *   node scripts/generate-protocol-typescript.mjs --check  # fail if stale
 */

import { readFile, writeFile } from "node:fs/promises";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const snapshotPath = resolve(root, "crates/oo-protocol/tests/snapshots/contract_schemas.json");
const outputPath = resolve(root, "web/packages/schema/src/protocol.generated.ts");

const raw = JSON.parse(await readFile(snapshotPath, "utf8"));

/** Hoist per-type $defs into one flat map, exactly like the OpenAPI layer. */
function flatten(snapshot) {
  const out = new Map();
  const put = (name, value) => {
    const encoded = JSON.stringify(value);
    if (out.has(name) && JSON.stringify(out.get(name)) !== encoded) {
      throw new Error(`conflicting protocol schema: ${name}`);
    } else if (!out.has(name)) {
      out.set(name, value);
    }
  };
  for (const [name, schema] of Object.entries(snapshot)) {
    const { $schema, title, $defs = {}, ...rest } = schema;
    put(name, rest);
    for (const [defName, def] of Object.entries($defs)) {
      const { $schema: _ignored, title: _t, ...defRest } = def;
      put(defName, defRest);
    }
  }
  return out;
}

const flat = flatten(raw);

function tsDocComment(description) {
  if (!description) return "";
  return description
    .split("\n")
    .map((line) => `/** ${line.trim()} */`)
    .join("\n");
}

function tsTypeOf(schema) {
  if (schema === undefined || Object.keys(schema).length === 0) {
    // Free-form JSON value (serde_json::Value).
    return "unknown";
  }
  if (schema.$ref) {
    return schema.$ref.replace(/^#\/\$defs\//, "");
  }
  if (Array.isArray(schema.enum)) {
    return schema.enum.map((value) => JSON.stringify(value)).join(" | ");
  }
  if (schema.type === "array") {
    return `Array<${tsTypeOf(schema.items)}>`;
  }
  switch (schema.type) {
    case "string":
      return "string";
    case "integer":
    case "number":
      return "number";
    case "boolean":
      return "boolean";
    case "object":
      return "Record<string, unknown>";
    default:
      throw new Error(`unsupported schema shape: ${JSON.stringify(schema)}`);
  }
}

function emitType(name, schema) {
  const doc = tsDocComment(schema.description);
  if (schema.enum || schema.type !== "object") {
    return `${doc}${doc ? "\n" : ""}export type ${name} = ${tsTypeOf(schema)};\n`;
  }
  const required = new Set(schema.required ?? []);
  const properties = Object.entries(schema.properties ?? {});
  if (properties.length === 0) {
    return `${doc}${doc ? "\n" : ""}export type ${name} = ${tsTypeOf(schema)};\n`;
  }
  const lines = properties.map(([field, fieldSchema]) => {
    const fieldDoc = tsDocComment(fieldSchema.description);
    const optional = required.has(field) ? "" : "?";
    return `${fieldDoc}${fieldDoc ? "\n" : ""}  ${field}${optional}: ${tsTypeOf(fieldSchema)};`;
  });
  return `${doc}${doc ? "\n" : ""}export interface ${name} {\n${lines.join("\n")}\n}\n`;
}

const order = [...flat.keys()].sort();
const header = `// @generated — AUTO-GENERATED from the Rust protocol contract (ADR-0010).
// Do not edit by hand. Regenerate with:
//   node scripts/generate-protocol-typescript.mjs
// Source: crates/oo-protocol/tests/snapshots/contract_schemas.json
`;
const body = order.map((name) => emitType(name, flat.get(name))).join("\n");
const content = `${header}\n${body}`;

if (process.argv.includes("--check")) {
  const committed = await readFile(outputPath, "utf8");
  if (committed !== content) {
    console.error("stale generated protocol types: web/packages/schema/src/protocol.generated.ts");
    console.error("Run: node scripts/generate-protocol-typescript.mjs");
    process.exitCode = 1;
  } else {
    console.log(`Protocol TypeScript types up to date (${order.length} types).`);
  }
} else {
  await writeFile(outputPath, content, "utf8");
  console.log(`Wrote ${order.length} protocol types to ${outputPath}`);
}
