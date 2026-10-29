import type { TFunction } from "i18next";
import type { GitRef } from "../../ipc";

/** "↑2 ↓1" for a branch's commits to push and to pull ("" when even). */
export function arrows(ref: Pick<GitRef, "ahead" | "behind">): string {
  const parts = [];
  if (ref.ahead) parts.push(`↑${ref.ahead}`);
  if (ref.behind) parts.push(`↓${ref.behind}`);
  return parts.join(" ");
}

/** Spelled-out tracking state of a local branch, for tooltips and screen
 * readers. */
export function trackingDescription(ref: GitRef, t: TFunction): string {
  if (!ref.upstream) return t("tracking.noUpstream");
  if (ref.upstreamGone) return t("tracking.gone", { upstream: ref.upstream });
  if (ref.ahead === null || ref.behind === null) {
    return t("tracking.tracks", { upstream: ref.upstream });
  }
  if (ref.ahead === 0 && ref.behind === 0) {
    return t("tracking.even", { upstream: ref.upstream });
  }
  return [
    t("tracking.tracks", { upstream: ref.upstream }),
    ref.ahead > 0 ? t("tracking.ahead", { count: ref.ahead }) : null,
    ref.behind > 0 ? t("tracking.behind", { count: ref.behind }) : null,
  ]
    .filter(Boolean)
    .join(" · ");
}
