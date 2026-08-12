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
if (stale) {
  console.error("Run: node scripts/generate-api-contract.mjs");
  process.exitCode = 1;
}
