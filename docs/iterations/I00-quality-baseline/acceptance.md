# I00 acceptance

## Required checks

- [ ] Capability matrix covers every current Document block kind and every exposed toolbar/menu action.
- [ ] `pnpm test:e2e` starts against an isolated server database and passes without a running developer server.
- [ ] Browser smoke proves an edit survives reload and revision increments exactly once.
- [ ] List, table selection, image navigation and menu closing tests initially encode current expected behavior.
- [ ] Visual suite captures both `office-light` and `office-dark` without animation or blinking cursor noise.
- [ ] CI uploads failure diagnostics and fails on browser regression.
- [ ] Existing Rust/TS quality gates remain unchanged and pass.

## Exit evidence

The iteration PR/report must include the matrix, a command output link for all gates, and a short list of capabilities still marked `partial` or `blocked`. It must not claim product parity.
