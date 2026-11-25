# Installing GitCRAFT

🌐 **English** · [Español](INSTALACION.md)

[macOS](#macos-apple-silicon) · [Windows](#windows) · [Linux](#linux)

## macOS (Apple Silicon)

GitCRAFT is distributed as a `.dmg` for Macs with Apple silicon (M1 or
later). **Tested on macOS 27.0.1; earlier versions are not verified.** **It is
not signed with an Apple developer certificate nor notarized** (see
[why](#why-isnt-gitcraft-signed) below): it only carries an *ad-hoc*
signature (local, with no identity), which Apple silicon needs before macOS
will run the app. That is why macOS asks for confirmation the first time you
open it. You do not need to disable Gatekeeper.

### Requirements

- Git installed: the one from Xcode's Command Line Tools
  (`xcode-select --install`) or from Homebrew (`brew install git`). GitCRAFT
  does not bundle its own Git.
- Git 2.30 or later. In the interactive rebase, editing messages and
  combining commits need Git 2.32 or later, and predicting conflicts exactly
  needs 2.40 or later; with an older Git, GitCRAFT says what is not available.
  To see which version you have: `git --version` in Terminal.
- To work with private remotes: a credential helper (on macOS, `osxkeychain`,
  which comes with Git) or an SSH key in the agent. GitCRAFT never stores or
  asks for passwords.
- For the GitHub account and "Publish on GitHub": GitHub CLI 2.81.0 or later
  (`brew install gh`; if you already have it, `brew upgrade gh`). To see your
  version: `gh --version`. GitCRAFT stores no tokens; gh keeps the session in the
  Keychain.

### Building the `.dmg`

```sh
npm install        # once
npm run dist
```

Run it on a Mac with Apple silicon: it builds for the machine's architecture
(arm64). The result is
`target/release/bundle/dmg/GitCRAFT_<version>_aarch64.dmg` (or under
`$CARGO_TARGET_DIR` if it is set).

### Installing

1. Download the `.dmg` from
   [Releases](https://github.com/epmoreno/GitCRAFT/releases) or build it
   yourself (above).
2. Double-click it to open it.
3. Drag **GitCRAFT** to the **Applications** folder.
4. Eject the "GitCRAFT" disk (⏏ button in Finder's sidebar).

### Opening it for the first time

**If you built the `.dmg` yourself on this Mac**, it usually opens without a
warning: macOS only asks for confirmation for what arrives downloaded
(browser, AirDrop, email, messaging…).

**If you downloaded it or someone sent it to you**, macOS blocks it the first
time because it cannot check who made it:

1. Open GitCRAFT from Applications (double-click). A warning says it could
   not be verified; press **OK** (or **Done**). Do not choose "Move to Trash".
2. Open **System Settings → Privacy & Security** and scroll down to the
   **Security** section. You will see a message about "GitCRAFT" with an
   **Open Anyway** button. Press it.
3. Confirm with your password or Touch ID and press **Open** on the last
   warning.

You only do this once: macOS remembers the exception **for this app only**.
Gatekeeper stays on for everything else.

Do not do any of this:

- `sudo spctl --master-disable` (or "Allow apps from: Anywhere"): it turns
  the protection off for every app.
- Removing attributes with `xattr` to skip the warning: the Settings step
  does the same for this app, visibly and reversibly.

### Why isn't GitCRAFT signed?

GitCRAFT is free, open-source software (MIT license): nothing is charged for
it. Signing and notarizing it with Apple requires a paid yearly Apple
Developer Program membership, and the project has chosen not to pay it. That
is why macOS shows a warning the first time you open it: it cannot check who
made the app.

To open it, follow
[Opening it for the first time](#opening-it-for-the-first-time): open the
app, press **OK** on the warning, then **System Settings → Privacy & Security
→ Open Anyway**. **Never disable Gatekeeper:** the exception applies to
GitCRAFT only.

#### How to check the app

Without Apple's signature, you can check what you download yourself:

- **SHA-256 checksum.** The notes of every GitHub Release include the `.dmg`'s
  SHA-256. Compute the one of your download and compare; they must be
  identical:

  ```sh
  shasum -a 256 ~/Downloads/GitCRAFT_<version>_aarch64.dmg
  ```

- **VirusTotal.** The notes of every Release link to VirusTotal's analysis of
  that same `.dmg`. Check that the SHA-256 VirusTotal shows is your file's.
- **Build it yourself.** The code is public: build the `.dmg` from source with
  `npm run dist` (see [Building the `.dmg`](#building-the-dmg)) and download
  nothing. A `.dmg` built by someone else has a different SHA-256 (it contains
  build dates and an *ad-hoc* signature), so building is an alternative to
  downloading, not a way to check a downloaded file.

### Uninstalling

1. Quit GitCRAFT (⌘Q).
2. Drag **GitCRAFT** from Applications to the Trash.
3. Optional, to also delete its settings (recents, open tabs, pull mode per
   repository, general settings and the changes panel view): in Finder,
   **Go → Go to Folder…** (⇧⌘G) and move these to the Trash, if they exist:
   - `~/Library/Application Support/dev.gitcraft.app`
   - `~/Library/Caches/dev.gitcraft.app`
   - `~/Library/WebKit/dev.gitcraft.app`

Uninstalling GitCRAFT **does not touch your repositories** or your Git
configuration: GitCRAFT stores nothing inside them, apart from what you do
with Git (commits, branches, stashes…).

Nor does it undo what you changed from Settings in git's or gh's
configuration, because that is your own configuration and still works in the
terminal:

- identity, initial branch and pull strategy (`~/.gitconfig` or each repo's
  `.git/config`);
- gh's credential helper, if you turned it on;
- the gh session.

## Windows

> **Not published yet:** the installer comes with v0.4.1.

For Windows 11, on x64 and ARM64. **It is not signed** (see
[why](#why-isnt-it-signed-on-windows) below), so Windows warns before opening
it. You do not need to disable any protection. The ARM64 one is only tested
automatically, in CI.

**What it needs:** [Git for Windows](https://git-scm.com/downloads/win)
(`winget install --id Git.Git`), 2.30 or later; GitCRAFT does not bundle its
own Git. `gh` is optional, for the GitHub features: GitHub CLI 2.81.0 or later
(`winget install --id GitHub.cli`; if you already have it, `winget upgrade --id
GitHub.cli`).

### Installing on Windows

1. Download `GitCRAFT_<version>_x64-setup.exe` (or `_arm64-setup.exe`, if your
   computer is ARM) from
   [Releases](https://github.com/epmoreno/GitCRAFT/releases).
2. Compare its SHA-256 with the one in the Release's notes (in PowerShell):

   ```powershell
   Get-FileHash "$HOME\Downloads\GitCRAFT_<version>_x64-setup.exe"
   ```

3. Open it and follow the steps. It installs for your user only, without
   administrator rights, in `%LOCALAPPDATA%\GitCRAFT`. If WebView2 is missing
   (Windows 11 already has it), it is downloaded from Microsoft.

### The SmartScreen warning

**What you see.** When you open the downloaded installer, a blue Microsoft
Defender SmartScreen window: "Windows protected your PC", which says it
prevented an unrecognized app from starting. Before that, the browser may warn
that the file is not commonly downloaded.

**How to open it, without turning anything off:**

1. Check the SHA-256 first (above).
2. If the browser warns, keep the file (in Edge, **… → Keep**).
3. In the blue window, press **More info**. The file's name and "Unknown
   publisher" appear: that is expected, because it is not signed.
4. Press **Run anyway**.

The exception is for that file only: SmartScreen stays on for everything else.
**Do not turn off** SmartScreen, Microsoft Defender or Smart App Control.

**If Smart App Control blocks it,** there is no button to go on: that Windows 11
protection blocks unsigned apps Microsoft does not know, and allows no
exception for a single one. With it on, GitCRAFT may not install; we do not
recommend turning it off for this.

### Why isn't it signed on Windows?

GitCRAFT is free, open-source software (MIT license), and a code-signing
certificate costs money every year. Without one, Windows cannot say who made
the installer. Signing it would not always remove the warning either:
SmartScreen also looks at how many people have already downloaded that file.
[Microsoft explains it here](https://learn.microsoft.com/windows/apps/package-and-deploy/smartscreen-reputation).
SignPath Foundation's free signing for open-source projects is still to be
looked into.

You can check what you download just as on macOS: SHA-256 and VirusTotal in
every Release's notes, or build it yourself with `npm run dist` (on Windows,
with [7-Zip](https://www.7-zip.org) installed, which it uses to check inside
the installer).

### Uninstalling on Windows

**Settings → Apps → Installed apps → GitCRAFT → Uninstall.** Tick **"Delete the
application data"** to also delete its settings and recents; it deletes no
repository.

## Linux

> **Not published yet:** the package comes with v0.4.1.

A `.deb` for Ubuntu 22.04 or later, on x64 (`amd64`) and ARM64 (`arm64`). The
ARM64 one is only tested automatically, in CI.

### Installing on Linux

1. Download `GitCRAFT_<version>_amd64.deb` (or `_arm64.deb`) from
   [Releases](https://github.com/epmoreno/GitCRAFT/releases).
2. Compare its SHA-256 with the one in the Release's notes:

   ```sh
   sha256sum GitCRAFT_<version>_amd64.deb
   ```

3. Install it with apt (with `./` in front), which adds whatever is missing,
   Git included:

   ```sh
   sudo apt install ./GitCRAFT_<version>_amd64.deb
   ```

The package is not signed and does not come from an apt repository: the
SHA-256 is the check.

`gh` is optional, for the GitHub features: GitHub CLI 2.81.0 or later, installed
from [its official repository](https://github.com/cli/cli/blob/trunk/docs/install_linux.md).
The one Ubuntu offers through apt is too old (2.46.0 in Ubuntu 26.04): GitCRAFT
detects it and says so. To see your version: `gh --version`.

### Uninstalling on Linux

`sudo apt remove git-craft`. The settings and recents stay in
`~/.config/dev.gitcraft.app`, `~/.local/share/dev.gitcraft.app` and
`~/.cache/dev.gitcraft.app` (those that exist): move them to the Trash if you
want them gone too. Your repositories and Git are not touched.
