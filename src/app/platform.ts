/**
 * The system the app runs on, as far as the interface cares: shortcuts and
 * their labels follow macOS conventions (⌘) or Windows/Linux ones (Ctrl).
 *
 * Read from the web view (`navigator.platform`: "MacIntel" in WKWebView,
 * "Win32" in WebView2, "Linux x86_64" in WebKitGTK). Tests choose it with
 * `setPlatformForTests`.
 */
export type Platform = "mac" | "other";

let forTests: Platform | null = null;

function detect(): Platform {
  const nav = typeof navigator === "undefined" ? undefined : navigator;
  const name = nav?.platform || nav?.userAgent || "";
  return /Mac|iPhone|iPad/.test(name) ? "mac" : "other";
}

const detected = detect();

export function platform(): Platform {
  return forTests ?? detected;
}

export function isMac(): boolean {
  return platform() === "mac";
}

/** Tests only: pretend to run on `value` (`null` goes back to detection). */
export function setPlatformForTests(value: Platform | null) {
  forTests = value;
}
