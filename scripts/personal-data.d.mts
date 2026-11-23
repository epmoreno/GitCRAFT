export interface Patterns {
  home: RegExp;
  name: RegExp | null;
}
export interface Found {
  file: string;
  home: number;
  name: number;
}
export function remapFlags(folders: {
  homes: string[];
  project: string;
  sysroot: string;
  cargoHome: string;
}): string[];
export function encodedRustflags(
  env: Record<string, string | undefined>,
  flags: string[],
): string;
export function isGenericName(name: string): boolean;
export function personalPatterns(who: {
  homes: string[];
  names: string[];
}): Patterns;
export function countPersonal(
  buffer: Uint8Array,
  patterns: Patterns,
): { home: number; name: number };
export function scan(
  root: string,
  label: string,
  patterns: Patterns,
): { checked: number; found: Found[] };
export function describe(found: Found): string;
