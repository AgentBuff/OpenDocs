import { spawnSync } from "node:child_process";
import { readFile } from "node:fs/promises";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { generatedFiles } from "./api-contract-source.mjs";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
let stale = false;
for (const [file, value] of Object.entries(generatedFiles())) {
  let actual;
  try {
    actual = await readFile(resolve(root, file), "utf8");
  } catch {
    console.error(`missing generated contract: ${file}`);
    stale = true;
    continue;
  }
  const expected = `${JSON.stringify(value, null, 2)}\n`;
  if (actual !== expected) {
    console.error(`stale generated contract: ${file}`);
    stale = true;
  }
}

// ADR-0010 phase 3: the generated TypeScript protocol types share the same
// Rust golden snapshot and must stay in lockstep with it.
const typescript = spawnSync(
  process.execPath,
  [resolve(root, "scripts/generate-protocol-typescript.mjs"), "--check"],
  { stdio: "inherit" },
);
if (typescript.status !== 0) stale = true;

if (stale) {
  console.error("Run: node scripts/generate-api-contract.mjs && node scripts/generate-protocol-typescript.mjs");
  process.exitCode = 1;
}
