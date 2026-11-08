# Installing GitCRAFT on macOS (Apple Silicon)

🌐 **English** · [Español](INSTALACION.md)

GitCRAFT is distributed as a `.dmg` for Macs with Apple silicon (M1 or
later). **Tested on macOS 27.0.1; earlier versions are not verified.** **It is
not signed with an Apple developer certificate nor notarized** (see
[why](#why-isnt-gitcraft-signed) below): it only carries an *ad-hoc*
signature (local, with no identity), which Apple silicon needs before macOS
will run the app. That is why macOS asks for confirmation the first time you
open it. You do not need to disable Gatekeeper.

## Requirements

- Git installed: the one from Xcode's Command Line Tools
  (`xcode-select --install`) or from Homebrew (`brew install git`). GitCRAFT
  does not bundle its own Git.
- To work with private remotes: a credential helper (on macOS, `osxkeychain`,
  which comes with Git) or an SSH key in the agent. GitCRAFT never stores or
  asks for passwords.
- For the GitHub account and "Publish on GitHub": GitHub CLI
  (`brew install gh`). GitCRAFT stores no tokens; gh keeps the session in the
  Keychain.

## Building the `.dmg`

```sh
npm install        # once
npm run dist
```

Run it on a Mac with Apple silicon: it builds for the machine's architecture
(arm64). The result is
`target/release/bundle/dmg/GitCRAFT_<version>_aarch64.dmg` (or under
`$CARGO_TARGET_DIR` if it is set).

## Installing

1. Download the `.dmg` from
   [Releases](https://github.com/epmoreno/GitCRAFT/releases) or build it
   yourself (above).
2. Double-click it to open it.
3. Drag **GitCRAFT** to the **Applications** folder.
4. Eject the "GitCRAFT" disk (⏏ button in Finder's sidebar).

## Opening it for the first time

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

## Why isn't GitCRAFT signed?

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

### How to check the app

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

## Uninstalling

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
