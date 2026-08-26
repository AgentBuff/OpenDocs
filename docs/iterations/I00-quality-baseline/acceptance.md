# I00 acceptance

## Required checks

- [x] Capability matrix covers every current Document block kind and every exposed toolbar/menu action.
  Evidence: `docs/quality/document-capability-matrix.md` now carries a "Block kind coverage"
  audit over all 15 schema kinds (incl. Page/Columns/Column containers and Extension/Unknown
  policy) plus a "Toolbar/menu action coverage" mapping gutter/format/table surfaces to rows
  (2026-08-26 audit).
- [x] `pnpm test:e2e` starts against an isolated server database and passes without a running developer server.
  Evidence: `web/playwright.config.ts` self-starts `oo-server` with `OO_DATA_DIR=.e2e-data`
  and a dedicated Vite instance on deterministic ports (`8788`/`5175`); full suite green
  locally on 2026-08-26 (14 passed).
- [x] Browser smoke proves an edit survives reload and revision increments exactly once.
  Evidence: `e2e/document/boot.spec.ts` reads `/api/artifacts/{id}` before the edit, asserts
  `revisionAfter === revisionBefore + 1` for one typed burst, then reloads and re-reads the
  text. Observed product semantics recorded: a structural key (Enter) flushes the autosave
  outbox immediately and therefore counts as its own transaction.
- [x] List, table selection, image navigation and menu closing tests initially encode current expected behavior.
  Evidence: `e2e/document/list.spec.ts`, `e2e/document/table-image.spec.ts`,
  `e2e/overlays/menu.spec.ts` (Escape/outside dismissal + portal stacking).
- [x] Visual suite captures both `office-light` and `office-dark` without animation or blinking cursor noise.
  Evidence: `e2e/visual/chrome.spec.ts` snapshots for toolbar and block menu in both themes;
  passing run 2026-08-26.
- [x] CI uploads failure diagnostics and fails on browser regression.
  Evidence: `.github/workflows/quality.yml` browser job uploads `playwright-report` and
  failure artifacts via `actions/upload-artifact@v4` under `if: failure()`; job failure fails CI.
- [x] Existing Rust/TS quality gates remain unchanged and pass.
  Evidence (2026-08-26): `cargo fmt --check` clean; `cargo test --workspace` 272 passed;
  `pnpm typecheck` OK; `pnpm architecture:check` pass (128 files); `pnpm test` 562 passed / 80 files.

## Exit evidence

Status: closed for the baseline scope. The matrix is maintained in
`docs/quality/document-capability-matrix.md`; every row remains honestly `partial` or
`planned` where noted — this iteration claims quality/interaction infrastructure, not
product parity. Gate outputs cited above were produced in one working session; CI runs the
same commands on every push.
