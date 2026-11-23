import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import {
  countPersonal,
  describe as describeFound,
  encodedRustflags,
  isGenericName,
  personalPatterns,
  remapFlags,
  scan,
} from "../../scripts/personal-data.mjs";

/* `npm run dist` must not publish whose computer made the app. The names
 * here are made up. */

const unix = personalPatterns({
  homes: ["/Users/anaperez"],
  names: ["anaperez"],
});
const windows = personalPatterns({
  homes: ["C:\\Users\\AnaPerez"],
  names: ["AnaPerez"],
});
const bytes = (text: string) => Buffer.from(text);

describe("the rustc flags", () => {
  it("remap the home folder first, so the more specific folders win", () => {
    expect(
      remapFlags({
        homes: ["/home/ana"],
        project: "/home/ana/my projects/GitCRAFT",
        sysroot: "/home/ana/.rustup/toolchains/stable",
        cargoHome: "/home/ana/.cargo",
      }),
    ).toEqual([
      "--remap-path-prefix=/home/ana=/home",
      "--remap-path-prefix=/home/ana/.rustup/toolchains/stable=/rust",
      "--remap-path-prefix=/home/ana/.cargo=/cargo",
      "--remap-path-prefix=/home/ana/my projects/GitCRAFT=/gitcraft",
    ]);
  });

  it("keep the ones already asked for, and allow spaces", () => {
    const flags = ["--remap-path-prefix=/a b=/c"];
    expect(encodedRustflags({}, flags)).toBe(flags[0]);
    expect(encodedRustflags({ RUSTFLAGS: "-C  debuginfo=0" }, flags)).toBe(
      `-C\x1fdebuginfo=0\x1f${flags[0]}`,
    );
    expect(
      encodedRustflags(
        { CARGO_ENCODED_RUSTFLAGS: "-C\x1fa b", RUSTFLAGS: "no" },
        flags,
      ),
    ).toBe(`-C\x1fa b\x1f${flags[0]}`);
  });
});

describe("the check for personal data", () => {
  it("finds the home folder and the name in a binary, whatever the case", () => {
    const file = bytes("\0/Users/anaperez/.cargo/x.rs\0/USERS/ANAPEREZ\0ok");
    expect(countPersonal(file, unix)).toEqual({ home: 2, name: 2 });
    expect(countPersonal(bytes("/cargo/registry/x.rs"), unix)).toEqual({
      home: 0,
      name: 0,
    });
  });

  it("finds Windows paths with either slash, or escaped", () => {
    const file = bytes(
      String.raw`c:\users\anaperez\a C:/Users/AnaPerez/b "C:\\Users\\AnaPerez"`,
    );
    expect(countPersonal(file, windows)).toEqual({ home: 3, name: 3 });
  });

  it("finds UTF-16 text, both ways round", () => {
    const le = Buffer.from("/Users/anaperez/x", "utf16le");
    const be = Buffer.from(le).swap16();
    expect(countPersonal(le, unix)).toEqual({ home: 1, name: 1 });
    expect(countPersonal(be, unix)).toEqual({ home: 1, name: 1 });
  });

  it("finds names with accents, composed or not", () => {
    const p = personalPatterns({
      homes: ["/home/anapérez"],
      names: ["anapérez"],
    });
    for (const form of ["NFC", "NFD"]) {
      const file = bytes("/home/anapérez/x".normalize(form));
      expect(countPersonal(file, p)).toEqual({ home: 1, name: 1 });
    }
  });

  it("does not take another folder or a common word for the user's", () => {
    const p = personalPatterns({ homes: ["/root"], names: ["root"] });
    expect(isGenericName("root")).toBe(true);
    expect(isGenericName("Runner")).toBe(true);
    expect(countPersonal(bytes("the root of /rootfs"), p)).toEqual({
      home: 0,
      name: 0,
    });
    expect(countPersonal(bytes("/root/.cargo"), p)).toEqual({
      home: 1,
      name: 0,
    });
  });

  it("says which file and how many times, never the name", () => {
    const dir = mkdtempSync(join(tmpdir(), "gitcraft-personal-"));
    onTestFinished(() => rmSync(dir, { recursive: true }));
    mkdirSync(join(dir, "Contents"));
    writeFileSync(join(dir, "Contents", "app"), "/Users/anaperez/x anaperez");
    writeFileSync(join(dir, "clean"), "nothing");
    writeFileSync(join(dir, "of-anaperez"), "");

    const { checked, found } = scan(dir, "App", unix);

    expect(checked).toBe(3);
    const lines = found.map(describeFound).sort();
    expect(lines).toHaveLength(2);
    expect(lines.join("\n")).not.toMatch(/anaperez/i);
    expect(lines[1]!.replaceAll("\\", "/")).toBe(
      "App/Contents/app: 1 × the home folder, 2 × the user name",
    );
    expect(scan(join(dir, "clean"), "clean", unix).found).toEqual([]);
  });
});
