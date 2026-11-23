// What `npm run dist` (dist.mjs) uses so that nothing it makes says whose
// computer made it: the rustc flags that take the paths out, and the check
// that none is left.
import { lstatSync, readdirSync, readFileSync, readlinkSync } from "node:fs";
import path from "node:path";

/** `--remap-path-prefix` flags for the folders rustc writes into the
 * executable (panic messages say the file they come from). The last one that
 * matches a path wins, so the home folder, which holds the others, goes
 * first. */
export function remapFlags({ homes, project, sysroot, cargoHome }) {
  const pairs = [
    ...homes.map((home) => [home, "/home"]),
    [sysroot, "/rust"],
    [cargoHome, "/cargo"],
    [project, "/gitcraft"],
  ];
  return pairs.map(([from, to]) => `--remap-path-prefix=${from}=${to}`);
}

/** The value of CARGO_ENCODED_RUSTFLAGS with `flags` after the ones the
 * environment already asked for. Unlike RUSTFLAGS, it allows spaces. */
export function encodedRustflags(env, flags) {
  const current = env.CARGO_ENCODED_RUSTFLAGS
    ? env.CARGO_ENCODED_RUSTFLAGS.split("\x1f")
    : (env.RUSTFLAGS ?? "").split(" ").filter(Boolean);
  return [...current, ...flags].join("\x1f");
}

/** User names that say nothing about anyone, and are ordinary words. */
const GENERIC = ["admin", "administrator", "runner", "runneradmin", "ubuntu"];

/** Too short or too common to look for by itself; the home folder is always
 * looked for. */
export function isGenericName(name) {
  return name.length < 5 || GENERIC.includes(name.toLowerCase());
}

const latin1 = (buffer) => buffer.toString("latin1").toLowerCase();

/** `text` as the bytes it may have in a file. */
function spellings(text) {
  const all = ["NFC", "NFD"].flatMap((form) => {
    const utf16 = Buffer.from(text.normalize(form), "utf16le");
    return [
      { bytes: Buffer.from(text.normalize(form), "utf8"), next: "" },
      { bytes: utf16, next: "" },
      { bytes: Buffer.from(utf16).swap16(), next: "\\0" },
    ];
  });
  const seen = new Set();
  return all.filter(({ bytes }) => {
    const key = bytes.toString("hex");
    return !seen.has(key) && seen.add(key);
  });
}

const escaped = (text) => text.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");

/** A pattern over the file read as lower case Latin-1. A whole one is not
 * followed by more of a name ("/root" is not in "/rootfs"). */
function pattern(texts, whole) {
  const options = texts
    .flatMap(spellings)
    .map(
      ({ bytes, next }) =>
        escaped(latin1(bytes)) + (whole ? `(?!${next}[a-z0-9_-])` : ""),
    );
  return new RegExp(options.join("|"), "g");
}

/** The patterns to look for: the home folder as each system and format may
 * write it, and the user names that are not generic. */
export function personalPatterns({ homes, names }) {
  const folders = homes.flatMap((home) => [
    home,
    home.replaceAll("\\", "/"),
    home.replaceAll("\\", "\\\\"),
  ]);
  const own = names.filter((name) => !isGenericName(name));
  return {
    home: pattern([...new Set(folders)], true),
    name: own.length > 0 ? pattern([...new Set(own)], false) : null,
  };
}

/** How many times `buffer` has the home folder and the user name. */
export function countPersonal(buffer, patterns) {
  const text = latin1(buffer);
  const count = (p) => (p ? (text.match(p)?.length ?? 0) : 0);
  return { home: count(patterns.home), name: count(patterns.name) };
}

/** Every file under `root` (or `root` itself), not following links. */
function filesUnder(root) {
  if (!lstatSync(root).isDirectory()) return [root];
  return readdirSync(root).flatMap((name) => filesUnder(path.join(root, name)));
}

/** The files under `root` with personal data, in their content, their path
 * or where they link to: `{ file, home, name }`, with the path relative to
 * `root` after `label`. */
export function scan(root, label, patterns) {
  const files = filesUnder(root);
  const found = files.flatMap((file) => {
    const relative = path.relative(root, file);
    const info = lstatSync(file);
    const content = info.isSymbolicLink()
      ? Buffer.from(readlinkSync(file))
      : info.isFile()
        ? readFileSync(file)
        : Buffer.alloc(0);
    const inside = countPersonal(content, patterns);
    const inPath = countPersonal(Buffer.from(relative), patterns);
    const home = inside.home + inPath.home;
    const name = inside.name + inPath.name;
    if (home === 0 && name === 0) return [];
    const shown = relative ? `${label}/${relative}` : label;
    return [{ file: hidden(shown, patterns), home, name }];
  });
  return { checked: files.length, found };
}

/** `text` without the personal data, to show it. */
function hidden(text, patterns) {
  if (countPersonal(Buffer.from(text), patterns).name === 0) return text;
  return "(a file whose own path has the user name)";
}

/** One line per file, which never says the name. */
export function describe({ file, home, name }) {
  return `${file}: ${home} × the home folder, ${name} × the user name`;
}
