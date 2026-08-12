#!/usr/bin/env node

/** Final local release checklist. Each check is explicit and can be run in CI or before tagging. */
import { execFileSync } from "node:child_process";
import { access } from "node:fs/promises";
import { resolve } from "node:path";

const root = resolve(new URL("..", import.meta.url).pathname);
const requiredDocs = [
  "docs/architecture/release-checklist.md",
  "docs/architecture/testing-contracts.md",
  "docs/performance.md",
  "CONTRIBUTING.md",
  "SECURITY.md",
];
for (const file of requiredDocs) {
  try { await access(resolve(root, file)); } catch { throw new Error(`缺少发布文档：${file}`); }
}
const commands = [
  ["generated-api-contract", "node", ["scripts/check-generated-api-contract.mjs"]],
  ["dependency-audit", "node", ["scripts/dependency-audit.mjs"]],
  ["rust-format", "cargo", ["fmt", "--all", "--", "--check"]],
];
for (const [name, command, args] of commands) {
  console.log(`== ${name} ==`);
  execFileSync(command, args, { cwd: root, stdio: "inherit" });
}
console.log("Release checklist static gates passed. Run web gates and CDP/visual smoke with a real browser session before tagging.");
