import { readdirSync, readFileSync } from "node:fs";
import en from "./en.json";
import es from "./es.json";

/*
 * GitCRAFT presents itself under its own name: public texts name no other Git
 * client. The project docs (CLAUDE.md and docs/) may name the open-source
 * clients whose code the clean-room rules allow consulting, but no
 * proprietary one.
 */
const ROOT = `${import.meta.dirname}/../../`;
const ES = "GitCRAFT – cliente Git visual, rápido y de código abierto";
const EN = "GitCRAFT – a visual, fast, open-source Git client";

const read = (path: string) => readFileSync(`${ROOT}${path}`, "utf8");

const uiSources = readdirSync(`${ROOT}src`, {
  recursive: true,
  encoding: "utf8",
})
  .filter((p) => /\.(ts|tsx|json|css)$/.test(p) && !p.includes("test"))
  .map((p) => `src/${p}`);

const PUBLIC_TEXTS = [
  "README.md",
  "docs/INSTALACION.md",
  "package.json",
  "index.html",
  "src-tauri/Cargo.toml",
  "src-tauri/tauri.conf.json",
  ...uiSources,
];

const PROJECT_DOCS = [
  "CLAUDE.md",
  "THIRD_PARTY_NOTICES.md",
  ...readdirSync(`${ROOT}docs`)
    .filter((p) => p.endsWith(".md"))
    .map((p) => `docs/${p}`),
];

const PROPRIETARY_CLIENTS =
  /git ?kraken|sublime ?merge|sourcetree|smart ?git|git ?fiend/i;
// With their capital only: both are everyday words.
const PROPRIETARY_CLIENTS_CASED = /\b(Fork|Tower)\b/;
const OPEN_SOURCE_CLIENTS =
  /sourcegit|github desktop|lazygit|tortoise ?git|\bmagit\b/i;

const proprietary = (line: string) =>
  PROPRIETARY_CLIENTS.test(line) || PROPRIETARY_CLIENTS_CASED.test(line);

const mentions = (text: string) =>
  text
    .split("\n")
    .filter((line) => proprietary(line) || OPEN_SOURCE_CLIENTS.test(line));

const proprietaryMentions = (text: string) =>
  text.split("\n").filter(proprietary);

describe("public identity", () => {
  it("scans the UI sources", () => {
    expect(PUBLIC_TEXTS).toContain("src/i18n/es.json");
    expect(PUBLIC_TEXTS).toContain("src/features/repo/StartPage.tsx");
  });

  it("mentions no other Git client in public texts", () => {
    const found = PUBLIC_TEXTS.flatMap((path) =>
      mentions(read(path)).map((line) => `${path}: ${line.trim()}`),
    );
    expect(found).toEqual([]);
  });

  it("names no proprietary Git client in the project docs", () => {
    expect(PROJECT_DOCS).toContain("docs/VISION.md");
    const found = PROJECT_DOCS.flatMap((path) =>
      proprietaryMentions(read(path)).map((line) => `${path}: ${line.trim()}`),
    );
    expect(found).toEqual([]);
  });

  it("recognises other clients, not everyday words", () => {
    expect(mentions("works like Sublime Merge")).toHaveLength(1);
    expect(mentions("a Tower of files")).toHaveLength(1);
    expect(mentions("code from SourceGit")).toHaveLength(1);
    expect(mentions("the tower and a fork in the road")).toEqual([]);
    expect(mentions("an open-source Git client")).toEqual([]);
    expect(proprietaryMentions("code from SourceGit (MIT)")).toEqual([]);
    expect(proprietaryMentions("unlike Fork")).toHaveLength(1);
  });

  it("uses the official description everywhere", () => {
    expect(es.app.description).toBe(ES);
    expect(en.app.description).toBe(EN);
    expect(JSON.parse(read("package.json")).description).toBe(ES);
    expect(read("src-tauri/Cargo.toml")).toContain(`description = "${ES}"`);
    const bundle = JSON.parse(read("src-tauri/tauri.conf.json")).bundle;
    expect(bundle.shortDescription).toBe(ES);
    expect(bundle.longDescription).toBe(ES);
    const readme = read("README.md");
    expect(readme).toContain(EN);
    expect(readme).toContain(ES);
    expect(read("docs/VISION.md")).toContain(ES);
  });
});
