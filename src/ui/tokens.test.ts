import { readdirSync, readFileSync } from "node:fs";

// Read from disk: Vitest does not load CSS contents into modules.
const SRC = `${import.meta.dirname}/../`;
const tokensCss = readFileSync(`${SRC}ui/tokens.css`, "utf8");

/*
 * A.1: the palette meets WCAG AA. The ratios are computed from tokens.css
 * itself, so changing a colour there re-checks every pair.
 */

type Rgba = { r: number; g: number; b: number; a: number };

function parseTokens(css: string): Map<string, string> {
  const root = css.replace(/\/\*[\s\S]*?\*\//g, "").match(/:root\s*{([^}]*)}/);
  const tokens = new Map<string, string>();
  for (const [, name, value] of (root?.[1] ?? "").matchAll(
    /--([a-z0-9-]+):\s*([^;]+);/g,
  )) {
    tokens.set(name!, value!.replace(/\s+/g, " ").trim());
  }
  return tokens;
}

const tokens = parseTokens(tokensCss);

/** Splits on commas that are not inside parentheses. */
function splitArgs(args: string): string[] {
  const parts: string[] = [];
  let depth = 0;
  let current = "";
  for (const ch of args) {
    if (ch === "(") depth++;
    if (ch === ")") depth--;
    if (ch === "," && depth === 0) {
      parts.push(current.trim());
      current = "";
    } else {
      current += ch;
    }
  }
  return [...parts, current.trim()];
}

function resolve(value: string): Rgba {
  const v = value.trim();
  let m: RegExpMatchArray | null;
  if ((m = v.match(/^var\(--([a-z0-9-]+)\)$/))) return token(m[1]!);
  if (v === "transparent") return { r: 0, g: 0, b: 0, a: 0 };
  if ((m = v.match(/^#([0-9a-f]{6})$/i))) {
    const n = parseInt(m[1]!, 16);
    return { r: n >> 16, g: (n >> 8) & 255, b: n & 255, a: 1 };
  }
  if ((m = v.match(/^rgb\((\d+) (\d+) (\d+) \/ (\d+)%\)$/))) {
    return { r: +m[1]!, g: +m[2]!, b: +m[3]!, a: +m[4]! / 100 };
  }
  if ((m = v.match(/^color-mix\(in srgb, (.*)\)$/))) {
    // Two colours with the first one's percentage, mixed with premultiplied
    // alpha as CSS Color 5 specifies.
    const [first, second] = splitArgs(m[1]!);
    const pm = first!.match(/^(.*) (\d+(?:\.\d+)?)%$/);
    if (!pm || !second) throw new Error(`unsupported color-mix: ${v}`);
    const p = +pm[2]! / 100;
    const a = resolve(pm[1]!);
    const b = resolve(second);
    const alpha = a.a * p + b.a * (1 - p);
    const channel = (k: "r" | "g" | "b") =>
      alpha === 0 ? 0 : (a[k] * a.a * p + b[k] * b.a * (1 - p)) / alpha;
    return { r: channel("r"), g: channel("g"), b: channel("b"), a: alpha };
  }
  throw new Error(`unsupported colour: ${v}`);
}

function token(name: string): Rgba {
  const value = tokens.get(name);
  if (!value) throw new Error(`token --${name} is not defined`);
  return resolve(value);
}

/** `fg` painted over the opaque `bg`. */
function over(fg: Rgba, bg: Rgba): Rgba {
  const k = (c: "r" | "g" | "b") => fg[c] * fg.a + bg[c] * (1 - fg.a);
  return { r: k("r"), g: k("g"), b: k("b"), a: 1 };
}

function luminance({ r, g, b }: Rgba): number {
  const lin = (c: number) => {
    const s = c / 255;
    return s <= 0.04045 ? s / 12.92 : ((s + 0.055) / 1.055) ** 2.4;
  };
  return 0.2126 * lin(r) + 0.7152 * lin(g) + 0.0722 * lin(b);
}

function contrast(fg: Rgba, bg: Rgba): number {
  const solid = over(fg, bg);
  const [hi, lo] = [luminance(solid), luminance(bg)].sort((x, y) => y - x);
  return (hi! + 0.05) / (lo! + 0.05);
}

const SURFACES = ["surface-0", "surface-1", "surface-2", "surface-3"];
/** Opaque state backgrounds. */
const STATE_BACKGROUNDS = ["selected-bg", "notice-bg", "hunk-bg", "warning-bg"];
/**
 * Translucent tints, checked over the surfaces they may sit on: 0 to 2
 * (surface-3 is for hover and raised controls, never under a tint).
 */
const TINTS = [
  "accent-tint",
  "diff-added-bg",
  "diff-removed-bg",
  "diff-picked-bg",
];

/** Every background text can be drawn on, as [label, colour]. */
const TEXT_BACKGROUNDS: [string, Rgba][] = [
  ...[...SURFACES, ...STATE_BACKGROUNDS].map(
    (n) => [n, token(n)] as [string, Rgba],
  ),
  ...TINTS.flatMap((tint) =>
    SURFACES.slice(0, 3).map(
      (s) => [`${tint} on ${s}`, over(token(tint), token(s))] as [string, Rgba],
    ),
  ),
];

const TEXT_TOKENS = [
  "text",
  "text-muted",
  "text-faint",
  "accent",
  "danger",
  "success",
  "warning",
  "info",
];
const LANES = Array.from({ length: 8 }, (_, i) => `lane-${i + 1}`);

function failures(pairs: [string, Rgba, string, Rgba][], min: number) {
  return pairs
    .map(([fgName, fg, bgName, bg]) => ({
      pair: `--${fgName} on ${bgName}`,
      ratio: Math.round(contrast(fg, bg) * 100) / 100,
    }))
    .filter((p) => p.ratio < min);
}

describe("design tokens: contrast (WCAG AA)", () => {
  it("text colours reach 4.5:1 on every background they can sit on", () => {
    const pairs = TEXT_TOKENS.flatMap((fg) =>
      TEXT_BACKGROUNDS.map(
        ([bg, colour]) =>
          [fg, token(fg), bg, colour] as [string, Rgba, string, Rgba],
      ),
    );
    expect(failures(pairs, 4.5)).toEqual([]);
  });

  it("selected text (always --text) reaches 4.5:1", () => {
    const pairs = SURFACES.map(
      (s) =>
        [
          "text",
          token("text"),
          `text-selection-bg on ${s}`,
          over(token("text-selection-bg"), token(s)),
        ] as [string, Rgba, string, Rgba],
    );
    expect(failures(pairs, 4.5)).toEqual([]);
  });

  it("text on filled buttons and chips reaches 4.5:1", () => {
    const pairs: [string, Rgba, string, Rgba][] = [
      ...["accent", "accent-strong", ...LANES].map(
        (bg) =>
          ["accent-contrast", token("accent-contrast"), bg, token(bg)] as [
            string,
            Rgba,
            string,
            Rgba,
          ],
      ),
      ["danger-contrast", token("danger-contrast"), "danger", token("danger")],
    ];
    expect(failures(pairs, 4.5)).toEqual([]);
  });

  it("control borders, the focus ring and graph lanes reach 3:1", () => {
    const pairs: [string, Rgba, string, Rgba][] = [
      ...["border-control", "accent"].flatMap((fg) =>
        [...SURFACES, "selected-bg"].map(
          (bg) =>
            [fg, token(fg), bg, token(bg)] as [string, Rgba, string, Rgba],
        ),
      ),
      ...LANES.flatMap((fg) =>
        ["surface-0", "selected-bg"].map(
          (bg) =>
            [fg, token(fg), bg, token(bg)] as [string, Rgba, string, Rgba],
        ),
      ),
    ];
    expect(failures(pairs, 3)).toEqual([]);
  });

  it("computes WCAG ratios", () => {
    const black = resolve("#000000");
    const white = resolve("#ffffff");
    expect(contrast(white, black)).toBeCloseTo(21, 5);
    expect(contrast(black, black)).toBe(1);
    // Half-transparent white over black is mid grey (5.32:1).
    expect(contrast({ ...white, a: 0.5 }, black)).toBeCloseTo(5.32, 1);
  });
});

/*
 * Every colour lives in tokens.css. Elsewhere only `var(--…)` is allowed;
 * the one exception is the per-chip lane tint, which mixes a variable that
 * is set on each chip and cannot be precomputed at :root.
 */
const sources = Object.fromEntries(
  readdirSync(SRC, { recursive: true, encoding: "utf8" })
    .filter((path) => /\.(css|ts|tsx)$/.test(path))
    .map((path) => [`/${path}`, readFileSync(`${SRC}${path}`, "utf8")]),
);

const COLOUR_LITERAL =
  /#[0-9a-f]{3,8}\b|\b(?:rgba?|hsla?|hwb|lab|lch|oklab|oklch)\(|color-mix\((?![^)]*var\(--lane\))/gi;

describe("design tokens: no colours outside tokens.css", () => {
  it("finds the sources to scan", () => {
    expect(Object.keys(sources).length).toBeGreaterThan(20);
    expect(Object.keys(sources)).toContain("/features/graph/GraphView.tsx");
  });

  it("recognises colour literals", () => {
    const found = (s: string) => (s.match(COLOUR_LITERAL) ?? []).length > 0;
    expect(found("color: #fff;")).toBe(true);
    expect(found('"#e08a4f"')).toBe(true);
    expect(found("box-shadow: 0 1px rgb(0 0 0 / 40%);")).toBe(true);
    expect(found("color-mix(in srgb, var(--accent) 7%, transparent)")).toBe(
      true,
    );
    expect(found("color-mix(in srgb, var(--lane) 22%, transparent)")).toBe(
      false,
    );
    expect(found('document.querySelector("#root")')).toBe(false);
    expect(found("color: var(--text);")).toBe(false);
  });

  it("components use tokens only", () => {
    const offenders = Object.entries(sources)
      .filter(
        ([path]) =>
          !path.endsWith("/ui/tokens.css") &&
          !/\.test\.tsx?$/.test(path) &&
          !path.includes("/test/"),
      )
      .flatMap(([path, text]) =>
        text
          .split("\n")
          .map((line, i) => ({ line, at: `${path}:${i + 1}` }))
          .filter(({ line }) => line.match(COLOUR_LITERAL))
          .map(({ line, at }) => `${at}: ${line.trim()}`),
      );
    expect(offenders).toEqual([]);
  });
});
