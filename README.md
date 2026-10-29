# GitCRAFT

A fast, visual desktop Git client. Tauri 2 + Rust + React + TypeScript. MIT licensed.

See [docs/VISION.md](docs/VISION.md), [docs/ROADMAP.md](docs/ROADMAP.md) and
[docs/ARCHITECTURE.md](docs/ARCHITECTURE.md).

## Requirements

- macOS (Apple Silicon) — other platforms later
- Rust ≥ 1.90, Node ≥ 22, git ≥ 2.30

## Commands

```sh
npm install          # once
npm run tauri dev    # run the app with hot reload
npm run check        # format + lint + typecheck + all tests (web and Rust)
npm run dist         # build GitCRAFT.app and .dmg into target/release/bundle
```
