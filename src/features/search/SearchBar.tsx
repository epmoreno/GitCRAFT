import { useEffect, useId, useRef } from "react";
import { useTranslation } from "react-i18next";
import type { GraphLine, SearchField } from "../../ipc";
import { isShortcut, whichShortcut } from "../../app/shortcuts";
import { useShortcutLabel } from "../../app/useShortcutLabel";
import { ErrorReport } from "../../ui/ErrorReport";
import { Tooltip } from "../../ui/Tooltip";
import { useBranchStore } from "../branches/store";
import { useGraphStore } from "../graph/store";
import { loadedMatches, nextMatch, positionOf } from "./navigate";
import { hashProblem, searchOf, useSearchStore } from "./store";
import styles from "./search.module.css";

const FIELDS: SearchField[] = ["message", "author", "hash", "path"];
/** Typing pauses this long before searching. */
export const SEARCH_DELAY_MS = 250;

/** The last line of git's message for a bad regular expression. */
function gitReason(details: string | null): string {
  const line = (details ?? "").split("\n").find((l) => l.startsWith("fatal: "));
  return line?.slice("fatal: ".length) ?? "";
}

/**
 * The history search bar (task 12.2), above the graph. ⌘F (Ctrl+F) opens
 * it while the history is on screen (`available`); the results are shown
 * in the graph and ⌘G / ⇧⌘G move between them, towards older commits.
 */
export function SearchBar({
  repoId,
  lines,
  graphDone,
  loadId,
  selectedOid,
  available,
}: {
  repoId: number;
  lines: readonly GraphLine[];
  graphDone: boolean;
  /** Changes when the history reloads: the search runs again. */
  loadId: number;
  selectedOid: string | null;
  /** The history is on screen (no diff or conflict view over it). */
  available: boolean;
}) {
  const { t } = useTranslation();
  const label = useShortcutLabel();
  const search = useSearchStore((s) => searchOf(s.searches, repoId));
  const store = useSearchStore.getState;
  const select = useGraphStore((s) => s.select);
  const inputRef = useRef<HTMLInputElement>(null);
  const errorId = useId();
  const hintId = useId();
  const { open, text, field, regex, matchCase, status, matches, found } =
    search;

  // Search after a pause in typing, and again when the history reloads.
  useEffect(() => {
    if (!open) return;
    const timer = setTimeout(
      () => void useSearchStore.getState().run(repoId),
      SEARCH_DELAY_MS,
    );
    return () => clearTimeout(timer);
  }, [open, text, field, regex, matchCase, loadId, repoId]);

  // Leaving the tab stops the search; it runs again on coming back.
  useEffect(() => () => useSearchStore.getState().stop(repoId), [repoId]);

  useEffect(() => {
    if (open && search.focusRequest > 0) {
      inputRef.current?.focus();
      inputRef.current?.select();
    }
  }, [open, search.focusRequest]);

  /** Moves to the next or previous result; waits if it is not loaded. */
  const jump = (direction: "next" | "previous") => {
    const now = searchOf(store().searches, repoId);
    if (now.found === 0) return;
    const from = selectedOid
      ? lines.findIndex((l) => l.oid === selectedOid)
      : -1;
    const start = from >= 0 ? from : direction === "next" ? -1 : lines.length;
    let index = nextMatch(lines, now.matches, start, direction);
    if (index < 0) {
      const waiting =
        direction === "next" &&
        (!graphDone || loadedMatches(lines, now.matches) < now.found);
      if (waiting) {
        store().setPending(repoId, "next");
        return;
      }
      // Wrap around, as when searching a document.
      index = nextMatch(
        lines,
        now.matches,
        direction === "next" ? -1 : lines.length,
        direction,
      );
    }
    const line = index >= 0 ? lines[index] : undefined;
    if (line) {
      store().setPending(repoId, null);
      select(repoId, line.oid);
    }
  };

  // A jump that waited for the graph: done once the result is loaded.
  const latestJump = useRef(jump);
  useEffect(() => {
    latestJump.current = jump;
  });
  useEffect(() => {
    if (search.pending) latestJump.current(search.pending);
  }, [search.pending, lines.length, search.version, graphDone]);

  // ⌘F opens the bar only while the history is on screen; ⌘G / ⇧⌘G move.
  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (!available) return;
      if (document.querySelector('[aria-modal="true"], [role="menu"]')) return;
      const id = whichShortcut(["find", "findNext", "findPrevious"], event);
      if (!id) return;
      if (id === "find") {
        event.preventDefault();
        store().openBar(repoId);
        return;
      }
      if (!searchOf(store().searches, repoId).open) return;
      event.preventDefault();
      latestJump.current(id === "findNext" ? "next" : "previous");
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [available, repoId, store]);

  if (!open) return null;

  const regexApplies = field === "message" || field === "author";
  const problem = field === "hash" && text.trim() ? hashProblem(text) : null;
  const running = status === "running";
  const position = positionOf(lines, matches, selectedOid);
  const total = `${found}${running ? "+" : ""}`;
  const counter =
    status === "idle"
      ? ""
      : status === "error"
        ? ""
        : found === 0
          ? running
            ? t("search.searching")
            : t("search.none")
          : position > 0
            ? t("search.position", { position, total })
            : t("search.results", { count: found, total });
  const percent =
    search.progressKnown && lines.length > 0
      ? Math.min(100, Math.round((search.examined / lines.length) * 100))
      : null;
  const invalid = search.error?.kind === "invalidPattern";

  return (
    <div
      className={styles.bar}
      role="search"
      aria-label={t("search.label")}
      onKeyDown={(event) => {
        if (isShortcut("escape", event)) {
          event.preventDefault();
          event.stopPropagation();
          store().close(repoId);
        }
      }}
    >
      <div className={styles.row}>
        <input
          ref={inputRef}
          type="search"
          className={styles.field}
          aria-label={t("search.field")}
          aria-invalid={invalid || !!problem}
          aria-describedby={invalid ? errorId : problem ? hintId : undefined}
          placeholder={t(`search.placeholder.${field}`)}
          value={text}
          spellCheck={false}
          autoComplete="off"
          onChange={(e) => store().setQuery(repoId, { text: e.target.value })}
          onKeyDown={(event) => {
            const id = whichShortcut(
              ["findNextInField", "findPreviousInField"],
              event,
            );
            if (!id) return;
            event.preventDefault();
            jump(id === "findNextInField" ? "next" : "previous");
          }}
        />
        <select
          className={styles.select}
          aria-label={t("search.in")}
          value={field}
          onChange={(e) =>
            store().setQuery(repoId, { field: e.target.value as SearchField })
          }
        >
          {FIELDS.map((f) => (
            <option key={f} value={f}>
              {t(`search.fieldName.${f}`)}
            </option>
          ))}
        </select>
        {regexApplies ? (
          <label className={styles.check}>
            <input
              type="checkbox"
              checked={regex}
              onChange={(e) =>
                store().setQuery(repoId, { regex: e.target.checked })
              }
            />
            {t("search.regex")}
          </label>
        ) : (
          <label className={styles.check}>
            <Tooltip label={t("search.regexOnly")}>
              <input type="checkbox" checked={false} disabled readOnly />
            </Tooltip>
            {t("search.regex")}
          </label>
        )}
        <label className={styles.check}>
          <input
            type="checkbox"
            checked={matchCase}
            onChange={(e) =>
              store().setQuery(repoId, { matchCase: e.target.checked })
            }
          />
          {t("search.matchCase")}
        </label>
      </div>
      <div className={styles.row}>
        <span className={styles.counter} aria-live="polite">
          {counter}
        </span>
        <Tooltip
          label={t("search.previous", { shortcut: label("findPrevious") })}
        >
          <button
            type="button"
            className={styles.button}
            aria-label={t("search.previousLabel")}
            disabled={found === 0}
            onClick={() => jump("previous")}
          >
            ↑
          </button>
        </Tooltip>
        <Tooltip label={t("search.next", { shortcut: label("findNext") })}>
          <button
            type="button"
            className={styles.button}
            aria-label={t("search.nextLabel")}
            disabled={found === 0}
            onClick={() => jump("next")}
          >
            ↓
          </button>
        </Tooltip>
        {running && (
          <>
            <progress
              className={styles.progress}
              max={100}
              value={percent ?? undefined}
              aria-label={t("search.progress")}
            />
            {percent !== null && (
              <span className={styles.percent}>{percent} %</span>
            )}
            <button
              type="button"
              className={styles.button}
              onClick={() => store().stop(repoId)}
            >
              {t("search.cancel")}
            </button>
          </>
        )}
        <span className={styles.spacer} />
        <Tooltip label={t("search.close")}>
          <button
            type="button"
            className={styles.button}
            aria-label={t("search.close")}
            onClick={() => store().close(repoId)}
          >
            ×
          </button>
        </Tooltip>
      </div>
      {problem && (
        <p id={hintId} className={styles.hint}>
          {t(`search.hash.${problem}`)}
        </p>
      )}
      {invalid && (
        <p id={errorId} className={styles.error} role="alert">
          {t("search.invalidPattern", {
            reason: gitReason(search.error?.details ?? null),
          })}
        </p>
      )}
      {search.error && !invalid && (
        <ErrorReport error={search.error} lead={t("search.failed")} />
      )}
      {search.outside && (
        <p className={styles.hint}>
          {t("search.outside", { short: search.outside.slice(0, 7) })}{" "}
          <button
            type="button"
            className={styles.link}
            onClick={() => {
              const oid = search.outside;
              if (!oid) return;
              useBranchStore
                .getState()
                .requestCreate(repoId, oid, oid.slice(0, 7));
            }}
          >
            {t("search.createBranch")}
          </button>
        </p>
      )}
    </div>
  );
}
