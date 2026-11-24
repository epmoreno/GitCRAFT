export const INSTALLERS: Record<
  "darwin" | "win32" | "linux",
  { bundles: string; folder: string; suffix: string }
>;
export function installersIn(
  bundle: string,
  version: string,
  platform: "darwin" | "win32" | "linux",
): string[];
export function unpackDeb(deb: string, into: string): void;
export function unpackNsis(installer: string, into: string): void;
