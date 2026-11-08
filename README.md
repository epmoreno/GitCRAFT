# GitCRAFT

GitCRAFT – a visual, fast, open-source Git client

GitCRAFT – cliente Git visual, rápido y de código abierto

> **Early version (v0.2.1).** GitCRAFT is usable day to day for the features
> listed below, but it is young: expect rough edges and missing features (see
> [what is not there yet](#not-there-yet)). Your repositories are safe:
> destructive operations always ask first, and GitCRAFT runs your own `git`.

Built with Tauri 2, Rust, React and TypeScript. MIT licensed.

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

## Screenshots

<!-- TODO: screenshots -->

_Coming soon._

## What works

- **Repositories**: open, recents, tabs (reorder, restore, shortcuts), a new
  tab (+ or ⌘T) with the start page, initialize, and clone with progress and
  cancellation.
- **Commit graph** for large repositories, with commit details and diffs.
- **Uncommitted changes**: stage and unstage files, hunks and lines; list or
  folder tree view with full keyboard navigation; stage or unstage a whole
  folder; commit and amend; discard with confirmation and undo.
- **Branches**: create, rename, delete (listing commits that would be left
  without a branch), checkout with stash-or-cancel; sidebar filter.
- **Stash**: list, create, apply, pop, drop with confirmation, recover a
  single file.
- **Remotes**: fetch, pull (merge or rebase) and push with progress and
  cancel, ahead/behind indicators, forced push only with
  `--force-with-lease`; optional automatic fetch.
- **Settings** (⚙ or ⌘,): language, text size, default clone folder; Git's
  default branch and pull strategy and your Git identity (global or per
  repository), always showing the exact `git config` command first.
- **GitHub** through [GitHub CLI](https://cli.github.com) (`gh`): see who is
  signed in, sign in inside the app, and publish a local repository in your
  account (private by default) after a summary of what will be uploaded.
  GitCRAFT never stores or reads a token.
- **Clear errors and an operations panel** with the exact history of the `git`
  and `gh` commands it ran (credentials redacted).
- Spanish and English UI; a softer dark palette that meets WCAG AA contrast.

### Not there yet

Merge and rebase from the UI, conflict resolution, cherry-pick, revert, reset,
tags, history search, file history and blame, submodules, worktrees, LFS and a
light theme are planned. The next phases are in
[docs/ROADMAP.md](docs/ROADMAP.md) (in Spanish).

## Install

**For now there is only an installer for macOS** (Apple Silicon). Windows and
Linux are planned: the code is being made portable first, then their
installers.

1. Get `GitCRAFT_<version>_aarch64.dmg` (until downloads are published, build
   it as explained below).
2. Open it and drag GitCRAFT to Applications.

It needs Git installed (Xcode Command Line Tools or Homebrew). GitHub features
(account, publishing) need GitHub CLI: `brew install gh`.

Tested on macOS 27.0.1; earlier versions are not verified.

The app is **not signed with an Apple developer certificate nor notarized**,
so macOS asks for confirmation the first time. How to open it without
disabling Gatekeeper, and how to uninstall it:
[docs/INSTALACION.md](docs/INSTALACION.md) (in Spanish).

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

More about the project: [docs/VISION.md](docs/VISION.md) (product vision) and
[docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) (design), in Spanish.

## License

[MIT](LICENSE). Third-party notices:
[THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md).
