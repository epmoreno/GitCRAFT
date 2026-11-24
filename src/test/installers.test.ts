import { spawnSync } from "node:child_process";
import {
  mkdirSync,
  mkdtempSync,
  readFileSync,
  rmSync,
  writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import {
  INSTALLERS,
  installersIn,
  unpackDeb,
  unpackNsis,
} from "../../scripts/installers.mjs";
import { personalPatterns, scan } from "../../scripts/personal-data.mjs";

/* The installers: what their configuration says, and the check inside them.
 * The names here are made up. */

const ROOT = `${import.meta.dirname}/../../`;
const read = (file: string) => readFileSync(`${ROOT}${file}`, "utf8");
const config = (file: string) => JSON.parse(read(`src-tauri/${file}`));

function temporary(): string {
  const dir = mkdtempSync(join(tmpdir(), "gitcraft-installers-"));
  onTestFinished(() => rmSync(dir, { recursive: true, force: true }));
  return dir;
}

describe("the configuration of the installers", () => {
  const mac = config("tauri.conf.json");
  const windows = config("tauri.windows.conf.json");
  const linux = config("tauri.linux.conf.json");

  it("makes the formats npm run dist publishes", () => {
    expect(mac.bundle.targets.join()).toBe(INSTALLERS.darwin.bundles);
    expect(windows.bundle.targets.join()).toBe(INSTALLERS.win32.bundles);
    expect(linux.bundle.targets.join()).toBe(INSTALLERS.linux.bundles);
  });

  it("says no one's name or email", () => {
    for (const { bundle } of [windows, linux]) {
      expect(bundle.publisher).toBe("GitCRAFT");
      expect(JSON.stringify(bundle)).not.toContain("@");
    }
    // Cargo's authors would be the .deb's Maintainer.
    for (const file of ["Cargo.toml", "src-tauri/Cargo.toml"])
      expect(read(file)).not.toMatch(/^authors/m);
  });

  it("is not signed, and changes only the bundle", () => {
    for (const platform of [windows, linux])
      expect(Object.keys(platform).sort()).toEqual(["$schema", "bundle"]);
    expect(JSON.stringify(windows)).not.toMatch(
      /signCommand|certificateThumbprint|timestampUrl/,
    );
  });

  it("installs for one user on Windows, in Spanish and English", () => {
    const { nsis } = windows.bundle.windows;
    expect(nsis.installMode).toBe("currentUser");
    expect(nsis.languages).toEqual(["English", "Spanish"]);
  });

  it("declares on Linux what the app starts", () => {
    expect(linux.bundle.linux.deb.depends).toEqual(["git", "xdg-utils"]);
  });

  it("has the same version everywhere", () => {
    const { version } = JSON.parse(read("package.json"));
    expect(mac.version).toBe(version);
    expect(/^version = "(.+)"$/m.exec(read("Cargo.toml"))?.[1]).toBe(version);
  });
});

describe("what npm run dist made", () => {
  it("is the installers of that version, for that system", () => {
    const bundle = temporary();
    const make = (folder: string, name: string) => {
      mkdirSync(join(bundle, folder), { recursive: true });
      writeFileSync(join(bundle, folder, name), "");
      return join(bundle, folder, name);
    };
    const exe = make("nsis", "GitCRAFT_0.4.1_x64-setup.exe");
    make("nsis", "GitCRAFT_0.4.0_x64-setup.exe");
    make("nsis", "GitCRAFT_0.4.1_x64-setup.nsis.zip");
    const deb = make("deb", "GitCRAFT_0.4.1_amd64.deb");
    make("deb", "GitCRAFT_0.4.10_amd64.deb");

    expect(installersIn(bundle, "0.4.1", "win32")).toEqual([exe]);
    expect(installersIn(bundle, "0.4.1", "linux")).toEqual([deb]);
    expect(installersIn(bundle, "0.4.1", "darwin")).toEqual([]);
  });

  it("is never taken as checked when it cannot be unpacked", () => {
    const dir = temporary();
    const file = join(dir, "not-an-installer");
    writeFileSync(file, "nothing");
    expect(() => unpackNsis(file, join(dir, "exe"))).toThrow();
    expect(() => unpackDeb(file, join(dir, "deb"))).toThrow();
  });
});

describe.runIf(process.platform === "linux")("the check inside a .deb", () => {
  const patterns = personalPatterns({
    homes: ["/home/anaperez"],
    names: ["anaperez"],
  });

  function deb(maintainer: string, content: string) {
    const dir = temporary();
    const tree = join(dir, "tree");
    mkdirSync(join(tree, "DEBIAN"), { recursive: true });
    mkdirSync(join(tree, "usr", "bin"), { recursive: true });
    writeFileSync(join(tree, "usr", "bin", "gitcraft"), content);
    writeFileSync(
      join(tree, "DEBIAN", "control"),
      `Package: git-craft\nVersion: 1\nArchitecture: all\nMaintainer: ${maintainer}\nDescription: test\n`,
    );
    const file = join(dir, "test.deb");
    const built = spawnSync("dpkg-deb", ["--build", tree, file]);
    expect(built.status, String(built.stderr)).toBe(0);
    const into = join(dir, "unpacked");
    mkdirSync(into);
    unpackDeb(file, into);
    return {
      raw: scan(file, "deb", patterns).found,
      inside: scan(into, "deb", patterns).found.map((f) =>
        f.file.replace(/^deb\//, ""),
      ),
    };
  }

  it("finds what the compression hides, and the metadata", () => {
    const filler = "nothing here ".repeat(500);
    const found = deb(
      "Ana <anaperez@example.invalid>",
      `${filler}/home/anaperez/x`,
    );
    expect(found.raw).toEqual([]);
    expect(found.inside.sort()).toEqual([
      "control.tar",
      "data.tar",
      "files/DEBIAN/control",
      "files/usr/bin/gitcraft",
    ]);
  });

  it("finds nothing in a clean one", () => {
    const found = deb("GitCRAFT", "/cargo/registry/x.rs");
    expect(found.inside).toEqual([]);
  });
});
