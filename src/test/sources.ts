import { readdirSync } from "node:fs";

/** The `src/` folder, ending in "/". */
export const SRC_DIR = `${import.meta.dirname}/../`;

/**
 * Every file under `src/`, relative to it, always with "/" as separator
 * (`readdirSync` gives "\" on Windows), for tests that read the sources.
 */
export function srcFiles(): string[] {
  return readdirSync(SRC_DIR, { recursive: true, encoding: "utf8" }).map((p) =>
    p.replaceAll("\\", "/"),
  );
}
