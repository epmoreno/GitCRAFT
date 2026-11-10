import { readFileSync } from "node:fs";
import { SRC_DIR as SRC, srcFiles } from "../test/sources";

// Read from disk: Vitest does not load CSS contents into modules.
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
const STATE_BACKGROUNDS = [
  "selected-bg",
  "selected-inactive-bg",
  "notice-bg",
  "hunk-bg",
  "warning-bg",
];
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

/** `[label, colour]` of a background, plus its hovered and pressed looks. */
function withStates(label: string, colour: Rgba): [string, Rgba][] {
  return [
    [label, colour],
    [`${label} + hover`, over(token("hover-overlay"), colour)],
    [`${label} + pressed`, over(token("pressed-overlay"), colour)],
  ];
}

/** Backgrounds of things that can be hovered and pressed: controls and rows. */
const INTERACTIVE_BACKGROUNDS = [
  ...SURFACES,
  "selected-bg",
  "selected-inactive-bg",
];

/**
 * Every background text can be drawn on, as [label, colour]. Banners,
 * hunk headers and tints are not interactive themselves (their buttons sit
 * on a surface), so only their resting look is checked.
 */
const TEXT_BACKGROUNDS: [string, Rgba][] = [
  ...INTERACTIVE_BACKGROUNDS.flatMap((n) => withStates(n, token(n))),
  ...STATE_BACKGROUNDS.filter((n) => !INTERACTIVE_BACKGROUNDS.includes(n)).map(
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
  it("text colours reach 4.5:1 on every background, also hovered and pressed", () => {
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
    const on = (fg: string, backgrounds: [string, Rgba][]) =>
      backgrounds.map(
        ([label, colour]) =>
          [fg, token(fg), label, colour] as [string, Rgba, string, Rgba],
      );
    const pairs = [
      // Filled buttons in every state; lane chips are not interactive.
      ...on("accent-contrast", [
        ...withStates("accent", token("accent")),
        ...withStates("accent-strong", token("accent-strong")),
        ...LANES.map((bg) => [bg, token(bg)] as [string, Rgba]),
      ]),
      ...on("danger-contrast", withStates("danger", token("danger"))),
    ];
    expect(failures(pairs, 4.5)).toEqual([]);
  });

  it("control borders, the focus ring and graph lanes reach 3:1 (also hovered and pressed)", () => {
    const pairs: [string, Rgba, string, Rgba][] = [
      ...["border-control", "focus-ring"].flatMap((fg) =>
        INTERACTIVE_BACKGROUNDS.flatMap((bg) =>
          withStates(bg, token(bg)).map(
            ([label, colour]) =>
              [fg, token(fg), label, colour] as [string, Rgba, string, Rgba],
          ),
        ),
      ),
      ...LANES.flatMap((fg) =>
        ["surface-0", "selected-bg", "selected-inactive-bg"].map(
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
  srcFiles()
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
          // Asks the engine whether it supports color-mix(): a feature
          // test, not a colour of the interface.
          path !== "/app/smoke.ts" &&
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

/*
 * A.2: interaction states. Hover and pressed are a background-image layer
 * (tokens.css); a component state rule that used the `background` shorthand
 * would wipe it, and an `outline: none` without another visible cue would
 * hide the keyboard focus.
 */
function cssRules(css: string): { selector: string; body: string }[] {
  return [
    ...css.replace(/\/\*[\s\S]*?\*\//g, "").matchAll(/([^{}]+)\{([^{}]*)\}/g),
  ].map(([, selector, body]) => ({
    selector: selector!.trim(),
    body: body!,
  }));
}

const STATE_SELECTOR =
  /:(hover|active|focus|focus-visible|focus-within|disabled)|\[aria-(current|selected|pressed)|Selected|selected|Active|Open\b/;

const modules = Object.entries(sources).filter(([path]) =>
  path.endsWith(".module.css"),
);

describe("design tokens: interaction states", () => {
  it("state rules set background-color, not the background shorthand", () => {
    const offenders = modules.flatMap(([path, css]) =>
      cssRules(css)
        .filter(
          (r) =>
            STATE_SELECTOR.test(r.selector) &&
            /(^|;)\s*background:/.test(r.body),
        )
        .map((r) => `${path}: ${r.selector}`),
    );
    expect(offenders).toEqual([]);
  });

  it("removing the focus outline always leaves another visible cue", () => {
    const offenders = modules.flatMap(([path, css]) =>
      cssRules(css)
        .filter(
          (r) =>
            /outline:\s*(none|0)\b/.test(r.body) &&
            !/(background-color|border-color|box-shadow):/.test(r.body),
        )
        .map((r) => `${path}: ${r.selector}`),
    );
    expect(offenders).toEqual([]);
  });

  it("defines the shared hover, pressed, focus and disabled rules", () => {
    const rules = cssRules(tokensCss);
    const has = (selector: RegExp, body: RegExp) =>
      rules.some((r) => selector.test(r.selector) && body.test(r.body));
    expect(has(/:hover$/, /background-image: var\(--hover-layer\)/)).toBe(true);
    expect(has(/:active$/, /background-image: var\(--pressed-layer\)/)).toBe(
      true,
    );
    expect(
      has(/^:focus-visible$/, /outline: 2px solid var\(--focus-ring\)/),
    ).toBe(true);
    expect(has(/:disabled/, /opacity: var\(--disabled-opacity\)/)).toBe(true);
  });
});
