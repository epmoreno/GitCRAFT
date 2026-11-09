/**
 * The system the app runs on, as far as the interface cares: shortcuts and
 * their labels follow macOS conventions (⌘) or Windows/Linux ones (Ctrl),
 * and a few texts (how to install git or gh) depend on it.
 *
 * Read from the web view (`navigator.platform`: "MacIntel" in WKWebView,
 * "Win32" in WebView2, "Linux x86_64" in WebKitGTK). Tests choose it with
 * `setPlatformForTests`.
 */
export type Os = "mac" | "windows" | "linux";

/** For shortcuts: macOS or the rest. */
export type Platform = "mac" | "other";

let forTests: Os | null = null;

function detect(): Os {
  const nav = typeof navigator === "undefined" ? undefined : navigator;
  const name = nav?.platform || nav?.userAgent || "";
  if (/Mac|iPhone|iPad/.test(name)) return "mac";
  if (/Win/.test(name)) return "windows";
  return "linux";
}

const detected = detect();

export function os(): Os {
  return forTests ?? detected;
}

export function platform(): Platform {
  return os() === "mac" ? "mac" : "other";
}

export function isMac(): boolean {
  return os() === "mac";
}

/**
 * The text key for this system: `key_windows` or `key_linux` when `exists`
 * says there is one, otherwise `key` (macOS texts have no suffix).
 */
export function osKey(key: string, exists: (key: string) => boolean): string {
  const system = os();
  if (system === "mac") return key;
  const specific = `${key}_${system}`;
  return exists(specific) ? specific : key;
}

/** Tests only: pretend to run on `value` (`null` goes back to detection). */
export function setPlatformForTests(value: Os | null) {
  forTests = value;
}
