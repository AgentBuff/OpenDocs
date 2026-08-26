# I00 — quality baseline and truthful capability inventory

## Objective

Make the statement “a Document capability is complete” testable. This iteration creates no new user-facing editing feature. It establishes the capability inventory, browser test harness, visual baselines and failure taxonomy used by all later iterations.

## In scope

- A machine-readable and human-readable capability matrix for Document.
- Playwright-based browser tests against the real editor and server.
- Stable seeded Document fixtures for text, list, image, table, code and nested blocks.
- Screenshot baselines for light/dark toolbar, menus, table selection and image selection/crop.
- CI gates that run the browser smoke suite and publish screenshots on failure.
- A defect classification that separates engine/model, interaction, overlay, persistence and visual regressions.

## Out of scope

- Changing Document schema or adding commands.
- Rebuilding a table/image interaction during this iteration.
- Implementing collaboration, comments or new insertion types.
- Treating visual snapshots as a replacement for semantic or browser behavior tests.

## Dependencies

- The canonical routes remain `/api/artifacts/**`.
- The browser starts the Rust server at `127.0.0.1:8787` and Vite at `127.0.0.1:5174`.
- Existing test fixtures must use strict v4 snapshots only; no legacy model fixture may be introduced.

## Invariants

1. Test setup creates artifacts through the public API or canonical typed fixtures, never by directly mutating SQLite.
2. A browser test may inspect the DOM, but writes must be observed as committed semantic transactions or persisted snapshot changes.
3. Every user-visible command in the capability matrix names its engine command(s), not only its toolbar control.
4. A capability is **complete** only if every required evidence column is green; otherwise it is `partial`, `planned`, or `blocked`.
