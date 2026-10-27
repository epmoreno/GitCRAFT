import { detectLanguage, resources } from "./index";

function keyPaths(value: unknown, prefix = ""): string[] {
  if (typeof value !== "object" || value === null) return [prefix];
  return Object.entries(value).flatMap(([key, child]) =>
    keyPaths(child, prefix ? `${prefix}.${key}` : key),
  );
}

describe("i18n", () => {
  it("has the same keys in every language", () => {
    const es = keyPaths(resources.es.translation).sort();
    const en = keyPaths(resources.en.translation).sort();
    expect(en).toEqual(es);
  });

  it("has no empty translations", () => {
    for (const { translation } of Object.values(resources)) {
      for (const path of keyPaths(translation)) {
        const text = path
          .split(".")
          .reduce<unknown>(
            (node, key) => (node as Record<string, unknown>)[key],
            translation,
          );
        expect(text, path).not.toBe("");
      }
    }
  });

  it("detects Spanish locales and falls back to English", () => {
    expect(detectLanguage("es-ES")).toBe("es");
    expect(detectLanguage("es")).toBe("es");
    expect(detectLanguage("en-US")).toBe("en");
    expect(detectLanguage("fr-FR")).toBe("en");
    expect(detectLanguage(undefined)).toBe("en");
  });
});
