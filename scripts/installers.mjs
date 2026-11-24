// What `npm run dist` (dist.mjs) makes on each system, and how to see inside
// the installers of Windows and Linux, whose content goes compressed.
import { spawnSync } from "node:child_process";
import {
  closeSync,
  existsSync,
  mkdirSync,
  openSync,
  readdirSync,
} from "node:fs";
import path from "node:path";

/** Per system: what `tauri build --bundles` makes, and the file to publish
 * (`<product>_<version>_<architecture><suffix>` in `bundle/<folder>`). */
export const INSTALLERS = {
  darwin: { bundles: "app,dmg", folder: "dmg", suffix: ".dmg" },
  win32: { bundles: "nsis", folder: "nsis", suffix: "-setup.exe" },
  linux: { bundles: "deb", folder: "deb", suffix: ".deb" },
};

/** The installers of `version` in the bundle folder `release/bundle`. */
export function installersIn(bundle, version, platform) {
  const { folder, suffix } = INSTALLERS[platform];
  const dir = path.join(bundle, folder);
  if (!existsSync(dir)) return [];
  return readdirSync(dir)
    .filter((f) => f.startsWith(`GitCRAFT_${version}_`) && f.endsWith(suffix))
    .map((f) => path.join(dir, f));
}

function run(command, args, stdout = "pipe") {
  const result = spawnSync(command, args, {
    encoding: "utf8",
    stdio: ["ignore", stdout, "pipe"],
    windowsHide: true,
  });
  if (result.error) throw result.error;
  if (result.status !== 0) {
    throw new Error(`${command} ${args[0]} failed: ${result.stderr}`);
  }
}

/** The two archives of a .deb unpacked (`files`), and as they are but not
 * compressed (`*.tar`), which also says who owns each file. */
export function unpackDeb(deb, into) {
  const files = path.join(into, "files");
  run("dpkg-deb", ["--raw-extract", deb, files]);
  if (!existsSync(path.join(files, "DEBIAN", "control"))) {
    throw new Error("the .deb was not unpacked: nothing to check");
  }
  for (const [option, name] of [
    ["--fsys-tarfile", "data.tar"],
    ["--ctrl-tarfile", "control.tar"],
  ]) {
    const out = openSync(path.join(into, name), "w");
    try {
      run("dpkg-deb", [option, deb], out);
    } finally {
      closeSync(out);
    }
  }
}

/** 7-Zip, which can unpack an NSIS installer without running it. */
function sevenZip() {
  const candidates = ["7z", "7z.exe"];
  for (const base of [process.env.ProgramFiles, process.env.ProgramW6432]) {
    if (base) candidates.push(path.join(base, "7-Zip", "7z.exe"));
  }
  const found = candidates.find((command) => {
    const result = spawnSync(command, ["i"], {
      stdio: "ignore",
      windowsHide: true,
    });
    return !result.error && result.status === 0;
  });
  if (!found) {
    throw new Error("7-Zip is needed to look inside the installer");
  }
  return found;
}

/** The files an NSIS installer carries, without installing it. */
export function unpackNsis(installer, into) {
  const files = path.join(into, "files");
  mkdirSync(files, { recursive: true });
  run(sevenZip(), ["x", "-y", "-bso0", "-bsp0", `-o${files}`, installer]);
  if (!existsSync(path.join(files, "gitcraft.exe"))) {
    throw new Error("the installer was not unpacked: nothing to check");
  }
}
