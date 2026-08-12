# Presentation node UI registry

`@open-office/presentation-ui` is the UI composition boundary for Presentation v5 nodes. It is deliberately framework-neutral: the future Canvas/WebGL stage, DOM text overlay and Inspector can consume its render plans and descriptors without storing or changing a `Deck`.

Each registration owns exactly one node type and provides renderer input, selection adornments, toolbar descriptors, Inspector fields, and action mapping. An action mapper returns typed semantic commands only; the future PresentationStore/server transaction boundary validates and executes them.

The built-in registry supports `text`, `shape`, `image`, `group`, and read-only `extension`. The Presentation Studio consumes this registry for selection adornments, capability-filtered contextual toolbar actions and the Inspector. Other v5 nodes resolve to an explicit unsupported Inspector with no toolbar actions. This is intentional: unsupported UI must never look actionable.

This package must not import the editor app, a renderer, or the Presentation engine.
