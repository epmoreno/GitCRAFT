# GitCRAFT

A fast, visual desktop Git client for macOS on Apple Silicon. Built with
Tauri 2, Rust, React and TypeScript. MIT licensed.

GitCRAFT makes Git's history and operations easy to read and quick to run,
without hiding what Git does underneath:

- **The commit graph is the centre**: colour lanes per branch, branch/tag/HEAD
  labels, smooth with tens of thousands of commits.
- **Safe by default**: every destructive operation asks first and explains
  what will be lost; discards can be undone.
- **Understandable errors**: Git's errors are explained in plain language, with
  Git's original output always one click away.
- **Your Git, your setup**: it runs the system `git` binary with your config,
  hooks, credential helper and SSH keys. GitCRAFT never stores credentials.
- **No AI features.**

More in [docs/VISION.md](docs/VISION.md) (product vision),
[docs/ROADMAP.md](docs/ROADMAP.md) (tasks and status) and
[docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) (design). The docs are in
Spanish.

## Screenshots

<!-- TODO: screenshots -->

_Coming soon: commit graph, staging by hunk/line, branches and stash, the
operations panel._

## Status

**v0.1.0: the MVP (phase 1) is complete and manually verified.** It includes:

- Repositories: open, recents, tabs (reorder, restore, shortcuts), initialize
  and clone with progress and cancellation.
- Commit graph for large repositories, commit details and diffs.
- Working directory: stage/unstage files, hunks and lines; commit and amend;
  discard with confirmation and undo.
- Branches: create, rename, delete (listing commits that would be left without
  a branch), checkout with stash-or-cancel; sidebar filter.
- Stash: list, create, apply, pop, drop with confirmation, recover a single
  file.
- Remotes: fetch, pull (merge or rebase) and push with progress and cancel,
  ahead/behind indicators, forced push only with `--force-with-lease`.
- Clear error messages with Git's original output (copyable), and an
  operations panel with the exact history of Git commands (credentials
  redacted).
- Spanish and English UI.

Next phases are listed in [docs/ROADMAP.md](docs/ROADMAP.md).

## Install

Download or build `GitCRAFT_<version>_aarch64.dmg`, open it and drag GitCRAFT
to Applications. It needs macOS 11 or later on Apple Silicon, and Git
installed (Xcode Command Line Tools or Homebrew).

The MVP build is **not signed with an Apple developer certificate nor
notarized**, so macOS asks for confirmation the first time. How to open it
without disabling Gatekeeper, and how to uninstall it:
[docs/INSTALACION.md](docs/INSTALACION.md).

## Build from source

Requirements: macOS on Apple Silicon, Rust ≥ 1.90, Node ≥ 22, git ≥ 2.30.

```sh
npm install          # once
npm run tauri dev    # run the app with hot reload
npm run check        # format + lint + typecheck + all tests (web and Rust)
npm run dist         # build GitCRAFT.app and the .dmg
```

`npm run dist` writes `target/release/bundle/macos/GitCRAFT.app` and
`target/release/bundle/dmg/GitCRAFT_<version>_aarch64.dmg`.

## License

[MIT](LICENSE). Third-party notices: [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md).
