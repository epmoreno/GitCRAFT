const UNITS: [Intl.RelativeTimeFormatUnit, number][] = [
  ["year", 365 * 24 * 3600],
  ["month", 30 * 24 * 3600],
  ["week", 7 * 24 * 3600],
  ["day", 24 * 3600],
  ["hour", 3600],
  ["minute", 60],
];

const formatters = new Map<string, Intl.RelativeTimeFormat>();

/** "hace 3 días" / "3 days ago" for a Unix time, relative to `nowMs`. */
export function relativeTime(
  unixSeconds: number,
  nowMs: number,
  language: string,
): string {
  let formatter = formatters.get(language);
  if (!formatter) {
    formatter = new Intl.RelativeTimeFormat(language, { numeric: "auto" });
    formatters.set(language, formatter);
  }
  const elapsed = unixSeconds - Math.floor(nowMs / 1000);
  for (const [unit, seconds] of UNITS) {
    if (Math.abs(elapsed) >= seconds) {
      return formatter.format(Math.trunc(elapsed / seconds), unit);
    }
  }
  return formatter.format(0, "second");
}

/** Full local date and time, for tooltips. */
export function absoluteTime(unixSeconds: number, language: string): string {
  return new Date(unixSeconds * 1000).toLocaleString(language, {
    dateStyle: "medium",
    timeStyle: "short",
  });
}
