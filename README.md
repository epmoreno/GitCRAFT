# GitCRAFT

GitCRAFT – a visual, fast, open-source Git client

GitCRAFT – cliente Git visual, rápido y de código abierto

For macOS on Apple Silicon. Built with Tauri 2, Rust, React and TypeScript. MIT
licensed.

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

_Coming soon: commit graph, the changes panel (list and tree), Settings, and
publishing on GitHub._

## Status

**v0.2.0: phase 1.5 (polish, settings and GitHub publishing) is complete
and manually verified**, on top of the v0.1.0 MVP.

New in 0.2.0:

- **Look and feel**: a softer graphite/copper palette that meets WCAG AA
  contrast (checked by tests), and consistent hover, pressed, selected,
  keyboard-focus and disabled states everywhere. Tooltips always stay inside
  the window.
- **Uncommitted changes panel**: header with "N changes on <branch>", the
  commit box fixed at the bottom with a resizable divider, collapsible
  Unstaged/Staged sections with their actions always visible, list or folder
  tree view (with counts, expand/collapse all and full keyboard navigation),
  sort by name or status, stage or unstage a whole folder, and ⌘⌫ to discard
  the focused file (with confirmation and undo).
- **New tab** (+ or ⌘T) with the start page: open, clone, initialize or pick
  a recent repository right there.
- **Settings** (⚙ or ⌘,):
  - General: language, text size (scales the whole UI), default folder for
    clones, automatic fetch every N minutes (off by default).
  - Git's own defaults, written to your global git config after showing the
    exact command and the previous value: default branch, default pull
    strategy.
  - Git identity, global and per repository, showing where each value comes
    from.
  - GitHub account through [GitHub CLI](https://cli.github.com) (`gh`): see
    who is signed in, sign in inside the app with the one-time code, sign
    out, and optionally let git use gh for github.com credentials.
    **GitCRAFT never stores or reads a token.**
  - A list of every keyboard shortcut.
- **Publish on GitHub**: for a repository without remotes, create it in your
  personal account (**private by default**) after a summary of what will be
  uploaded, with warnings for a missing `.gitignore`, files over 50 MB and
  committed build folders. It checks first that the name is free and never
  force-pushes.

From v0.1.0 (the MVP):

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
to Applications. It needs Apple Silicon and Git
installed (Xcode Command Line Tools or Homebrew). GitHub features (account,
publishing) need [GitHub CLI](https://cli.github.com): `brew install gh`.

Tested on macOS 27.0.1; earlier versions are not verified.

The build is **not signed with an Apple developer certificate nor
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
