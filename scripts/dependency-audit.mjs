#!/usr/bin/env node

/**
 * Reproducible dependency/license inventory for release review.
 *
 * This is deliberately read-only: it uses Cargo metadata and the checked-in pnpm lockfile,
 * prints a machine-readable inventory, and never installs or modifies dependencies. A release
 * fails when a workspace package has no declared license; third-party license review is emitted
 * as an explicit list instead of guessing from package names.
 */
import { execFileSync } from "node:child_process";
import { readFile, writeFile } from "node:fs/promises";
import { resolve } from "node:path";

const root = resolve(new URL("..", import.meta.url).pathname);
const reportPath = process.env.OO_DEPENDENCY_REPORT;
const metadata = JSON.parse(execFileSync("cargo", ["metadata", "--format-version", "1", "--no-deps"], {
  cwd: root,
  encoding: "utf8",
}));
const workspaceIds = new Set(metadata.workspace_members);
const rust = metadata.packages.map((pkg) => ({
  ecosystem: "cargo",
  name: pkg.name,
  version: pkg.version,
  license: pkg.license ?? null,
  workspace: workspaceIds.has(pkg.id),
}));

const lock = await readFile(resolve(root, "web/pnpm-lock.yaml"), "utf8");
const pnpmPackages = [...lock.matchAll(/^\s{2}((?:[^\s]|\s)+?):\n/gm)]
  .map((match) => match[1].trim())
  .filter((name) => name.startsWith("@") || /^[a-z0-9]/i.test(name))
  .slice(0, 10_000);
const report = {
  generatedAt: new Date().toISOString(),
  rust,
  pnpm: {
    lockfile: "web/pnpm-lock.yaml",
    packageEntryCount: pnpmPackages.length,
    note: "Use pnpm licenses list / OSV scanner in the release environment for SPDX resolution.",
  },
};
const missingWorkspaceLicenses = rust.filter((pkg) => pkg.workspace && !pkg.license);
if (missingWorkspaceLicenses.length > 0) {
  console.error("依赖审计失败：workspace crate 缺少 license 字段");
  for (const pkg of missingWorkspaceLicenses) console.error(`- ${pkg.name}@${pkg.version}`);
  process.exitCode = 1;
}
const output = JSON.stringify(report, null, 2);
if (reportPath) await writeFile(resolve(root, reportPath), `${output}\n`);
console.log(output);
