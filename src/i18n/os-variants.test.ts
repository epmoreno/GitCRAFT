import { resources } from "../i18n";

/*
 * Texts that depend on the system have a macOS base (`key`) and, where
 * Windows or Linux need other words, `key_windows` / `key_linux` (before the
 * plural suffix: `untracked_windows_one`). See `osKey` in app/platform.ts.
 */
const MACOS_ONLY =
  /Homebrew|Xcode|brew |osxkeychain|Llavero|Keychain|Finder|macOS/;

function flatten(value: unknown, prefix = ""): [string, string][] {
  if (typeof value === "string") return [[prefix, value]];
  return Object.entries(value as Record<string, unknown>).flatMap(([k, v]) =>
    flatten(v, prefix ? `${prefix}.${k}` : k),
  );
}

describe("texts per system", () => {
  for (const [language, { translation }] of Object.entries(resources)) {
    const texts = new Map(flatten(translation));
    const variants = [...texts.keys()].flatMap((key) => {
      const m = /^(.*)_(windows|linux)(_one|_other)?$/.exec(key);
      return m ? [{ key, base: `${m[1]}${m[3] ?? ""}`, system: m[2] }] : [];
    });

    it(`${language}: every variant has its macOS base text`, () => {
      expect(variants.length).toBeGreaterThan(0);
      for (const { key, base } of variants) {
        expect(texts.get(base), `${key} without ${base}`).toBeTruthy();
      }
    });

    it(`${language}: variants never name macOS things`, () => {
      for (const { key } of variants) {
        expect(texts.get(key), key).not.toMatch(MACOS_ONLY);
      }
    });

    it(`${language}: Windows variants never point to ~/.gitconfig`, () => {
      for (const { key, system } of variants) {
        if (system === "windows")
          expect(texts.get(key), key).not.toContain("~/");
      }
    });
  }
});
