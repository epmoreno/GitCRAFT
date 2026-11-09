import { readdirSync, readFileSync } from "node:fs";

/*
 * Phase P lets Rust tests be skipped on Windows only while their task is
 * pending: each one says its task (`#[cfg_attr(windows, ignore = "P.x: …")]`)
 * and is listed under "Tests ignorados en Windows" in docs/ROADMAP.md. The
 * list and the code must agree, and once P.10 is ticked none may remain.
 */
const ROOT = `${import.meta.dirname}/../../`;
const RUST_DIRS = ["crates", "src-tauri/src"];

interface Ignore {
  file: string;
  test: string;
  task: string;
}

const key = (i: Ignore) => `${i.file} · ${i.test} · ${i.task}`;

function rustFiles(): string[] {
  return RUST_DIRS.flatMap((dir) =>
    readdirSync(`${ROOT}${dir}`, { recursive: true, encoding: "utf8" })
      .map((p) => `${dir}/${p.replaceAll("\\", "/")}`)
      .filter((p) => p.endsWith(".rs") && !p.includes("/target/")),
  );
}

/** The Windows ignores in Rust `source`, wherever rustfmt breaks the lines. */
function windowsIgnores(file: string, source: string): Ignore[] {
  const attribute =
    /#\[cfg_attr\(\s*windows\s*,\s*ignore\s*=\s*"((?:[^"\\]|\\.)*)"\s*,?\s*\)\]\s*(?:#\[[^\]]*\]\s*)*(?:pub\s+)?(?:async\s+)?fn\s+(\w+)/g;
  return [...source.matchAll(attribute)].map(([, reason = "", test = ""]) => ({
    file,
    test,
    task: /^(P\.\d+):/.exec(reason)?.[1] ?? `no task in "${reason}"`,
  }));
}

/** Every `cfg_attr` that ignores something on Windows, in any form. */
function anyWindowsIgnore(source: string): number {
  return [...source.matchAll(/#\[cfg_attr\(([^\]]*)\)\]/g)].filter(
    ([, inner = ""]) => /windows/.test(inner) && /\bignore\b/.test(inner),
  ).length;
}

function roadmapList(roadmap: string): Ignore[] {
  const section =
    /### Tests ignorados en Windows\n([\s\S]*?)(?=\n## |\n### |$)/.exec(
      roadmap,
    )?.[1];
  if (section === undefined) return [];
  return [...section.matchAll(/^\| `(\w+)` \| `([^`]+)` \| (P\.\d+) \|/gm)].map(
    ([, test = "", file = "", task = ""]) => ({ file, test, task }),
  );
}

const roadmap = readFileSync(`${ROOT}docs/ROADMAP.md`, "utf8");
const sources = rustFiles().map((file) => ({
  file,
  source: readFileSync(`${ROOT}${file}`, "utf8"),
}));
const inCode = sources.flatMap(({ file, source }) =>
  windowsIgnores(file, source),
);

describe("tests ignored on Windows", () => {
  it("finds the attribute however rustfmt lays it out", () => {
    const source = [
      "#[tokio::test]",
      "#[cfg_attr(",
      "    windows,",
      '    ignore = "P.7: a \\"quoted\\" reason"',
      ")]",
      "async fn one() {}",
      '#[cfg_attr(windows, ignore = "no task")]',
      "fn two() {}",
    ].join("\n");
    expect(windowsIgnores("a.rs", source)).toEqual([
      { file: "a.rs", test: "one", task: "P.7" },
      { file: "a.rs", test: "two", task: 'no task in "no task"' },
    ]);
  });

  it("uses only the form this check understands", () => {
    for (const { file, source } of sources) {
      expect(anyWindowsIgnore(source), file).toBe(
        windowsIgnores(file, source).length,
      );
    }
  });

  it("names its phase P task in every reason", () => {
    for (const ignore of inCode)
      expect(ignore.task, key(ignore)).toMatch(/^P\.\d+$/);
  });

  it("matches the list in the roadmap", () => {
    expect(roadmapList(roadmap).map(key).sort()).toEqual(
      inCode.map(key).sort(),
    );
  });

  it("leaves none once P.10 is done", () => {
    if (/^- \[x\] \*\*P\.10\b/m.test(roadmap))
      expect(inCode.map(key)).toEqual([]);
  });
});
