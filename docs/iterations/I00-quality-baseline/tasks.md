# I00 file-level tasks

## I00-01 — inventory — complete

- Create `docs/quality/document-capability-matrix.md`.
- Populate all first-wave rows from `web/packages/schema/src/artifact.ts`, `crates/oo-document/src/lib.rs`, `web/apps/editor/src/blocks/`, and server command registration.
- Link each row to exact source and test evidence.
- Mark previously reported table/image/menu defects as `partial` until their E2E test passes.

## I00-02 — browser harness — complete

- Add `web/playwright.config.ts`.
- `web/playwright.config.ts` owns isolated server/Vite lifecycle; a separate process wrapper is intentionally unnecessary.
- Add `web/apps/editor/e2e/support/fixtures.ts` for public-API fixture creation.
- Add `web/apps/editor/e2e/support/selectors.ts`; selectors use roles, `data-block-id`, and stable labels only.
- Add root `web/package.json` scripts: `test:e2e`, `test:e2e:update`, `test:visual`.

## I00-03 — initial executable regressions — in progress

- Done: `e2e/document/boot.spec.ts`: load document, edit paragraph, reload, assert persisted content.
- Done: `e2e/document/list.spec.ts`: Enter continues list; empty-list Enter exits list and creates ordinary paragraph.
- Done: `e2e/overlays/menu.spec.ts`: outside click and Escape closing. Submenu closing remains pending with the new overlay coordinator.
- Add `e2e/table/selection.spec.ts`: edit single cell vs range selection vs toolbar visibility.
- Add `e2e/image/object.spec.ts`: select image, keyboard leave-left/right, insert before/after.

## I00-04 — visual baseline — in progress

- Done: `e2e/visual/chrome.spec.ts` for light/dark top toolbar and dark floating block menu.
- Add `e2e/visual/table.spec.ts` for cell, row, column and all-table selection.
- Add `e2e/visual/image.spec.ts` for selected and crop states.
- Store approved snapshots under Playwright’s conventional `*-snapshots/` directories; do not add binary snapshots outside test ownership.

## I00-05 — CI — complete

- Update `.github/workflows/quality.yml` with a browser job after web build.
- Install Playwright Chromium deterministically and cache only safe browser artifacts.
- Upload failure trace/screenshot/video as CI artifacts.
- Make browser job required before an iteration can be declared complete.

## I00-06 — defect workflow — complete

- Add `docs/quality/regression-policy.md`.
- Rule: reproduce → write failing E2E → fix semantic/UI code → retain regression fixture → add to capability matrix.
- Rule: a screenshot alone cannot close a defect involving persistence, keyboard, selection or context menu.
