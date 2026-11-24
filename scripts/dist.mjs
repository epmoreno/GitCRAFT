// `npm run dist`: builds the app to publish, without the paths of the folder
// it is built in, and checks that nothing in it says whose computer made it
// (personal-data.mjs). On macOS, GitCRAFT.app and the .dmg; on Windows, the
// NSIS installer; on Linux, the .deb. It looks inside the installers: on
// Windows that needs 7-Zip, and without it the check fails.
//
//   node scripts/dist.mjs --check-only          checks what is already built
//   node scripts/dist.mjs --check-folder <dir>  checks the files in a folder
//   node scripts/dist.mjs --must-find <file>    fails if the check sees
//       nothing in a file known to have personal data: the check works here
import { spawnSync } from "node:child_process";
import {
  existsSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  realpathSync,
  rmSync,
} from "node:fs";
import { homedir, tmpdir, userInfo } from "node:os";
import path from "node:path";
import {
  INSTALLERS,
  installersIn,
  unpackDeb,
  unpackNsis,
} from "./installers.mjs";
import {
  describe,
  encodedRustflags,
  isGenericName,
  personalPatterns,
  remapFlags,
  scan,
} from "./personal-data.mjs";

const project = realpathSync(path.join(import.meta.dirname, ".."));
const mac = process.platform === "darwin";
const { bundles } = INSTALLERS[process.platform];
const homes = [...new Set([homedir(), realpathSync(homedir())])];
const names = [...new Set([userInfo().username, path.basename(homedir())])];

function run(command, args, options = {}) {
  const result = spawnSync(command, args, { cwd: project, ...options });
  if (result.error) throw result.error;
  return result;
}

/** What `command` prints; it must succeed. */
function output(command, args) {
  const result = run(command, args, {
    encoding: "utf8",
    maxBuffer: 64 * 1024 * 1024,
  });
  if (result.status !== 0) {
    throw new Error(`${command} ${args[0]} failed: ${result.stderr}`);
  }
  return result.stdout.trim();
}

function build() {
  const flags = remapFlags({
    homes,
    project,
    sysroot: output("rustc", ["--print", "sysroot"]),
    cargoHome: process.env.CARGO_HOME ?? path.join(homedir(), ".cargo"),
  });
  const tauri = path.join(project, "node_modules/@tauri-apps/cli/tauri.js");
  // CI: the .dmg is made without asking Finder to arrange its window.
  const env = {
    ...process.env,
    CI: "true",
    CARGO_ENCODED_RUSTFLAGS: encodedRustflags(process.env, flags),
  };
  const result = run(process.execPath, [tauri, "build", "--bundles", bundles], {
    stdio: "inherit",
    env,
  });
  if (result.status !== 0) process.exit(result.status ?? 1);
}

/** The .dmg's content is compressed: looks at its files, and at the whole
 * disk image byte by byte (names, attributes and whatever is not a file). */
function scanDmg(dmg, label, patterns) {
  const temp = mkdtempSync(path.join(tmpdir(), "gitcraft-dist-"));
  const mount = path.join(temp, "mount");
  const raw = path.join(temp, "raw");
  let mounted = false;
  try {
    mkdirSync(mount);
    output("hdiutil", ["convert", dmg, "-quiet", "-format", "UDTO", "-o", raw]);
    output("hdiutil", [
      "attach",
      dmg,
      ...["-quiet", "-readonly", "-nobrowse", "-noautoopen"],
      ...["-mountpoint", mount],
    ]);
    mounted = true;
    return [
      scan(dmg, label, patterns),
      scan(`${raw}.cdr`, `${label} (disk image)`, patterns),
      scan(mount, `${label} (files)`, patterns),
    ];
  } finally {
    if (mounted) output("hdiutil", ["detach", "-quiet", mount]);
    rmSync(temp, { recursive: true, force: true });
  }
}

/** The .deb and the NSIS installer are compressed too: looks at them as they
 * are, and at what they carry. */
function scanUnpacked(installer, label, patterns, unpack) {
  const temp = mkdtempSync(path.join(tmpdir(), "gitcraft-dist-"));
  try {
    unpack(installer, temp);
    return [
      scan(installer, label, patterns),
      scan(temp, `${label} (inside)`, patterns),
    ];
  } finally {
    rmSync(temp, { recursive: true, force: true });
  }
}

const scanInstaller = {
  darwin: scanDmg,
  win32: (...args) => scanUnpacked(...args, unpackNsis),
  linux: (...args) => scanUnpacked(...args, unpackDeb),
}[process.platform];

function ownPatterns() {
  if (names.every(isGenericName)) {
    console.log("dist: the user name is too generic to look for by itself");
  }
  return personalPatterns({ homes, names });
}

/** Ends with an error if anything was found. */
function report(results) {
  const found = results.flatMap((r) => r.found);
  const checked = results.reduce((sum, r) => sum + r.checked, 0);
  if (checked === 0) throw new Error("nothing to check");
  if (found.length > 0) {
    console.error("dist: personal data in what would be published:");
    for (const one of found) console.error(`  ${describe(one)}`);
    console.error("dist: do not publish this build");
    process.exit(1);
  }
  console.log(`dist: no personal data in the ${checked} files checked`);
}

function check() {
  const metadata = JSON.parse(
    output("cargo", ["metadata", "--no-deps", "--format-version", "1"]),
  );
  const release = path.join(metadata.target_directory, "release");
  const { version } = JSON.parse(
    readFileSync(path.join(project, "package.json"), "utf8"),
  );
  const patterns = ownPatterns();

  // The interface goes compressed inside the executable: it is checked here.
  const places = [
    [path.join(project, "dist"), "dist"],
    [
      path.join(
        release,
        process.platform === "win32" ? "gitcraft.exe" : "gitcraft",
      ),
      "the executable",
    ],
  ];
  const results = [];
  if (mac) {
    places.push([
      path.join(release, "bundle/macos/GitCRAFT.app"),
      "GitCRAFT.app",
    ]);
  }
  for (const [place, label] of places) {
    if (!existsSync(place))
      throw new Error(`${label} is missing: nothing to check`);
    results.push(scan(place, label, patterns));
  }
  const installers = installersIn(
    path.join(release, "bundle"),
    version,
    process.platform,
  );
  if (installers.length === 0) {
    throw new Error(`there is no installer of ${version}`);
  }
  for (const installer of installers) {
    results.push(
      ...scanInstaller(installer, path.basename(installer), patterns),
    );
  }
  report(results);
}

function mustFind(file) {
  const { found } = scan(file, path.basename(file), ownPatterns());
  if (found.length === 0) {
    console.error("dist: the check saw nothing where there is personal data");
    process.exit(1);
  }
  for (const one of found) console.log(`dist: as expected, ${describe(one)}`);
}

const [mode, place] = process.argv.slice(2);
switch (mode) {
  case undefined:
    build();
    check();
    break;
  case "--check-only":
    check();
    break;
  case "--check-folder":
    report([scan(path.resolve(place), "the folder", ownPatterns())]);
    break;
  case "--must-find":
    mustFind(path.resolve(place));
    break;
  default:
    throw new Error(`unknown option ${mode}`);
}
