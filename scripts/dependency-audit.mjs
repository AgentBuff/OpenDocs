#!/usr/bin/env node

/**
 * Dependency inventory, license review and vulnerability gate.
 *
 * This is read-only: it never installs or modifies dependencies. It reports on the
 * *full* dependency graph (workspace crates plus every third-party crate resolved in
 * Cargo.lock, and the pnpm lockfile) rather than only the workspace members.
 *
 * Vulnerability scanning delegates to the ecosystem scanners (`cargo audit` for
 * RustSec advisories, `pnpm audit` for npm advisories). When a scanner is missing
 * the run reports that explicitly. Set OO_REQUIRE_DEPENDENCY_SCANNERS=1 (as CI does)
 * to turn a missing scanner into a failure instead of a warning.
 *
 * Exit code is non-zero when: a workspace crate declares no license, a third-party
 * crate declares no license, a vulnerability is reported, or a scanner is required
 * but unavailable.
 */
import { execFileSync } from "node:child_process";
import { readFile, writeFile } from "node:fs/promises";
import { resolve } from "node:path";

const root = resolve(new URL("..", import.meta.url).pathname);
const reportPath = process.env.OO_DEPENDENCY_REPORT;
const requireScanners = process.env.OO_REQUIRE_DEPENDENCY_SCANNERS === "1";

/** Licenses accepted without further review. Anything else is listed for a human. */
const ALLOWED_LICENSES = new Set([
  "MIT",
  "Apache-2.0",
  "Apache-2.0 WITH LLVM-exception",
  "BSD-2-Clause",
  "BSD-3-Clause",
  "ISC",
  "Zlib",
  "Unicode-3.0",
  "Unicode-DFS-2016",
  "CC0-1.0",
  "0BSD",
  "MPL-2.0",
]);

function tryRun(command, args, cwd = root) {
  try {
    return {
      ok: true,
      stdout: execFileSync(command, args, {
        cwd,
        encoding: "utf8",
        stdio: ["ignore", "pipe", "pipe"],
        // The resolved dependency graph is well over execFileSync's 1 MiB default.
        maxBuffer: 256 * 1024 * 1024,
      }),
      stderr: "",
    };
  } catch (error) {
    return { ok: false, stdout: error.stdout ?? "", stderr: error.stderr ?? "", status: error.status ?? 1 };
  }
}

// ---------------------------------------------------------------- Rust inventory
// No --no-deps: the point of this gate is the third-party graph, not our own crates.
const metadataResult = tryRun("cargo", ["metadata", "--format-version", "1", "--locked"]);
if (!metadataResult.ok) {
  console.error("依赖审计失败：无法解析 Cargo 元数据（Cargo.lock 可能已过期）");
  console.error(metadataResult.stderr.trim());
  process.exit(1);
}
const metadata = JSON.parse(metadataResult.stdout);
const workspaceIds = new Set(metadata.workspace_members);
const rust = metadata.packages.map((pkg) => ({
  ecosystem: "cargo",
  name: pkg.name,
  version: pkg.version,
  license: pkg.license ?? null,
  workspace: workspaceIds.has(pkg.id),
}));

const workspaceCrates = rust.filter((pkg) => pkg.workspace);
const thirdPartyCrates = rust.filter((pkg) => !pkg.workspace);

// ----------------------------------------------------------------- pnpm inventory
const lock = await readFile(resolve(root, "web/pnpm-lock.yaml"), "utf8");
const pnpmPackages = [...lock.matchAll(/^\s{2}((?:[^\s]|\s)+?):\n/gm)]
  .map((match) => match[1].trim())
  .filter((name) => name.startsWith("@") || /^[a-z0-9]/i.test(name))
  .slice(0, 10_000);

// ------------------------------------------------------------------- Rust licenses
const unlicensedWorkspace = workspaceCrates.filter((pkg) => !pkg.license);
const unlicensedThirdParty = thirdPartyCrates.filter((pkg) => !pkg.license);
const reviewLicenses = new Map();
for (const pkg of [...workspaceCrates, ...thirdPartyCrates]) {
  if (!pkg.license) continue;
  const accepted = pkg.license
    .split(/\s+(?:OR|AND)\s+|\//)
    .map((part) => part.trim())
    .some((part) => ALLOWED_LICENSES.has(part));
  if (!accepted) reviewLicenses.set(pkg.license, (reviewLicenses.get(pkg.license) ?? 0) + 1);
}

// ------------------------------------------------------------------ Rust advisories
const auditProbe = tryRun("cargo", ["audit", "--version"]);
let rustAdvisories = null;
let rustScannerError = null;
if (auditProbe.ok) {
  // `cargo audit` exits non-zero when it finds advisories; the JSON still lands on stdout.
  const result = tryRun("cargo", ["audit", "--json"]);
  try {
    const parsed = JSON.parse(result.stdout);
    rustAdvisories = (parsed.vulnerabilities?.list ?? []).map((entry) => ({
      id: entry.advisory?.id ?? null,
      package: entry.package?.name ?? null,
      version: entry.package?.version ?? null,
      title: entry.advisory?.title ?? null,
      severity: entry.advisory?.severity ?? null,
    }));
  } catch {
    rustScannerError = result.stderr.trim() || "cargo audit 输出无法解析";
  }
}

// ------------------------------------------------------------------- pnpm advisories
const pnpmProbe = tryRun("pnpm", ["--version"]);
let pnpmAdvisories = null;
let pnpmScannerError = null;

const parsePnpmAdvisories = (stdout) =>
  Object.values(JSON.parse(stdout).advisories ?? {}).map((entry) => ({
    id: entry.id ?? null,
    module: entry.module_name ?? null,
    severity: entry.severity ?? null,
    title: entry.title ?? null,
  }));

if (pnpmProbe.ok) {
  const webRoot = resolve(root, "web");
  const all = tryRun("pnpm", ["audit", "--json"], webRoot);
  // `pnpm audit --prod` is the only reliable dev/prod split: the advisory JSON carries
  // no per-finding `dev` flag, so an advisory absent from the production audit is
  // reachable only through devDependencies and cannot ship to users.
  const prod = tryRun("pnpm", ["audit", "--prod", "--json"], webRoot);
  if (!all.stdout.trim()) {
    pnpmScannerError = all.stderr.trim() || "pnpm audit 无输出（可能需要网络）";
  } else {
    try {
      const productionIds = new Set(parsePnpmAdvisories(prod.stdout).map((advisory) => advisory.id));
      pnpmAdvisories = parsePnpmAdvisories(all.stdout).map((advisory) => ({
        ...advisory,
        devOnly: !productionIds.has(advisory.id),
      }));
    } catch {
      pnpmScannerError = "pnpm audit 输出无法解析（可能需要网络）";
    }
  }
}

// ------------------------------------------------------------------------- report
const report = {
  generatedAt: new Date().toISOString(),
  rust: {
    workspaceCrates: workspaceCrates.length,
    thirdPartyCrates: thirdPartyCrates.length,
    packages: rust,
  },
  pnpm: {
    lockfile: "web/pnpm-lock.yaml",
    packageEntryCount: pnpmPackages.length,
    scanner: pnpmAdvisories === null ? "unavailable" : "pnpm audit",
  },
  licenses: {
    allowed: [...ALLOWED_LICENSES],
    needsReview: Object.fromEntries([...reviewLicenses.entries()].sort((a, b) => b[1] - a[1])),
  },
  advisories: { rust: rustAdvisories, pnpm: pnpmAdvisories },
};

const output = JSON.stringify(report, null, 2);
if (reportPath) await writeFile(resolve(root, reportPath), `${output}\n`);
console.log(output);

// ------------------------------------------------------------------------ verdict
let failed = false;

if (unlicensedWorkspace.length > 0) {
  console.error("\n依赖审计失败：workspace crate 缺少 license 字段");
  for (const pkg of unlicensedWorkspace) console.error(`- ${pkg.name}@${pkg.version}`);
  failed = true;
}
if (unlicensedThirdParty.length > 0) {
  console.error(`\n依赖审计失败：${unlicensedThirdParty.length} 个第三方 crate 未声明 license`);
  for (const pkg of unlicensedThirdParty.slice(0, 20)) console.error(`- ${pkg.name}@${pkg.version}`);
  failed = true;
}
if (reviewLicenses.size > 0) {
  console.error("\n需要人工复核的许可证（非允许列表）：");
  for (const [license, count] of reviewLicenses) console.error(`- ${license} × ${count}`);
}

if (rustAdvisories === null) {
  const message = rustScannerError ?? "未安装 cargo audit";
  if (requireScanners) {
    console.error(`\n依赖审计失败：Rust 漏洞扫描不可用（${message}）。安装：cargo install cargo-audit`);
    failed = true;
  } else {
    console.error(`\n警告：跳过 Rust 漏洞扫描（${message}）。安装：cargo install cargo-audit`);
  }
} else if (rustAdvisories.length > 0) {
  console.error(`\n依赖审计失败：cargo audit 报告 ${rustAdvisories.length} 个 RustSec 漏洞`);
  for (const advisory of rustAdvisories) {
    console.error(`- ${advisory.id} ${advisory.package}@${advisory.version} [${advisory.severity}] ${advisory.title}`);
  }
  failed = true;
}

if (pnpmAdvisories === null) {
  const message = pnpmScannerError ?? "pnpm 不可用";
  if (requireScanners) {
    console.error(`\n依赖审计失败：pnpm 漏洞扫描不可用（${message}）`);
    failed = true;
  } else {
    console.error(`\n警告：跳过 pnpm 漏洞扫描（${message}）`);
  }
} else {
  const blocking = pnpmAdvisories.filter((advisory) => !advisory.devOnly);
  const devOnly = pnpmAdvisories.filter((advisory) => advisory.devOnly);

  if (blocking.length > 0) {
    console.error(`\n依赖审计失败：pnpm audit 报告 ${blocking.length} 个生产依赖漏洞`);
    for (const advisory of blocking) {
      console.error(`- ${advisory.id} ${advisory.module} [${advisory.severity}] ${advisory.title}`);
    }
    failed = true;
  }
  if (devOnly.length > 0) {
    console.error(
      `\n警告：pnpm audit 报告 ${devOnly.length} 个仅 devDependencies 可达的漏洞（不随产物发布，建议顺手升级）：`,
    );
    for (const advisory of devOnly) {
      console.error(`- ${advisory.id} ${advisory.module} [${advisory.severity}] ${advisory.title}`);
    }
  }
}

if (failed) process.exitCode = 1;
