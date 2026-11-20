import { useEffect, useId, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import type { Expiry, ReflogEntry } from "../../ipc";
import { isShortcut } from "../../app/shortcuts";
import { ContextMenu, type MenuItem } from "../../ui/ContextMenu";
import { ErrorReport } from "../../ui/ErrorReport";
import dialogStyles from "../../ui/ConfirmDialog.module.css";
import { useBranchStore } from "../branches/store";
import { absoluteTime, relativeTime } from "../graph/relativeTime";
import { useGraphStore } from "../graph/store";
import { useHistoryStore } from "../history/store";
import { branchOf, useReflogStore } from "./store";
import styles from "./reflog.module.css";

/** "Lo anota git…", with the repository's own expiry settings. */
function Explained({
  reachable,
  unreachable,
}: {
  reachable: Expiry;
  unreachable: Expiry;
}) {
  const { t } = useTranslation();
  return (
    <p>
      {t("reflog.noted")}{" "}
      {unreachable.type === "never"
        ? t("reflog.unreachableNever")
        : t("reflog.unreachable", { count: unreachable.days })}{" "}
      {reachable.type === "never"
        ? t("reflog.reachableNever")
        : t("reflog.reachable", { count: reachable.days })}
    </p>
  );
}

export function ReflogDialog() {
  const { t, i18n } = useTranslation();
  const dialog = useReflogStore((s) => s.dialog);
  const close = useReflogStore((s) => s.close);
  const head = useGraphStore((s) =>
    dialog ? (s.graphs[dialog.repoId]?.refs?.head ?? null) : null,
  );
  const titleId = useId();
  const closeRef = useRef<HTMLButtonElement>(null);
  const listRef = useRef<HTMLUListElement>(null);
  const [menu, setMenu] = useState<{
    entry: ReflogEntry;
    x: number;
    y: number;
  } | null>(null);

  const open = !!dialog;
  useEffect(() => {
    if (!open) return;
    closeRef.current?.focus();
    const onKey = (event: KeyboardEvent) => {
      // Esc closes an entry's menu first (the menu handles it).
      if (document.querySelector('[role="menu"]')) return;
      if (isShortcut("escape", event)) {
        event.stopPropagation();
        close();
      }
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, [open, close]);

  if (!dialog) return null;
  const { repoId, refname, reflog, error } = dialog;
  const name = branchOf(refname);
  // The branch a reset would move: this one, or HEAD's.
  const current = head?.type === "branch" ? head.name : null;
  const target = refname === "HEAD" ? current : name;

  const items = (entry: ReflogEntry): MenuItem[] => [
    {
      label: t("reflog.show"),
      disabled: entry.onlyHere,
      hint: entry.onlyHere ? t("reflog.notInGraph") : undefined,
      onSelect: () => {
        close();
        useGraphStore.getState().select(repoId, entry.oid);
      },
    },
    {
      label: t("reflog.createBranch"),
      onSelect: () => {
        close();
        useBranchStore.getState().requestCreate(repoId, entry.oid, entry.short);
      },
    },
    {
      label: t("reflog.reset", { branch: target ?? "HEAD" }),
      danger: true,
      separatorBefore: true,
      disabled: !target || target !== current,
      hint: !current
        ? t("reflog.detached")
        : target !== current
          ? t("reflog.switchFirst", { branch: target })
          : undefined,
      onSelect: () => {
        close();
        void useHistoryStore.getState().requestReset(repoId, entry.oid);
      },
    },
  ];

  const openMenu = (entry: ReflogEntry, element: HTMLElement) => {
    const rect = element.getBoundingClientRect();
    setMenu({ entry, x: rect.right - 8, y: rect.bottom });
  };

  // Arrow keys move between entries; the menu key or ⇧F10 opens its menu.
  const onRowKey = (
    event: React.KeyboardEvent<HTMLButtonElement>,
    entry: ReflogEntry,
  ) => {
    const rows = Array.from(
      listRef.current?.querySelectorAll<HTMLButtonElement>("[data-row]") ?? [],
    );
    const index = rows.indexOf(event.currentTarget);
    if (isShortcut("menuNext", event)) {
      event.preventDefault();
      rows[Math.min(index + 1, rows.length - 1)]?.focus();
    } else if (isShortcut("menuPrevious", event)) {
      event.preventDefault();
      rows[Math.max(index - 1, 0)]?.focus();
    } else if (isShortcut("commitMenu", event)) {
      event.preventDefault();
      openMenu(entry, event.currentTarget);
    }
  };

  return (
    <div className={dialogStyles.backdrop}>
      <div
        className={`${dialogStyles.dialog} ${styles.dialog}`}
        role="dialog"
        aria-modal="true"
        aria-labelledby={titleId}
      >
        <h2 id={titleId} className={dialogStyles.title}>
          {refname === "HEAD"
            ? t("reflog.titleHead")
            : t("reflog.title", { branch: name })}
        </h2>
        <div className={dialogStyles.body}>
          {error && <ErrorReport error={error} lead={t("reflog.failed")} />}
          {!error && !reflog && <p>{t("reflog.loading")}</p>}
          {reflog && (
            <>
              <p>
                {refname === "HEAD"
                  ? t("reflog.leadHead")
                  : t("reflog.lead", { branch: name })}
              </p>
              <Explained
                reachable={reflog.expiry.reachable}
                unreachable={reflog.expiry.unreachable}
              />
              {reflog.entries.length === 0 ? (
                <p>{t("reflog.empty")}</p>
              ) : (
                <ul
                  ref={listRef}
                  className={styles.list}
                  aria-label={t("reflog.entries")}
                >
                  {reflog.entries.map((entry) => (
                    <li key={entry.index}>
                      <button
                        type="button"
                        data-row
                        className={styles.row}
                        aria-haspopup="menu"
                        onClick={(e) => openMenu(entry, e.currentTarget)}
                        onKeyDown={(e) => onRowKey(e, entry)}
                      >
                        <span
                          className={styles.when}
                          title={absoluteTime(entry.time, i18n.language)}
                        >
                          {relativeTime(
                            entry.time,
                            dialog.openedAt,
                            i18n.language,
                          )}
                        </span>
                        <code className={styles.oid}>{entry.short}</code>
                        <span className={styles.subject}>
                          «{entry.subject}»
                          {entry.onlyHere && (
                            <span className={styles.badge}>
                              {" "}
                              {t("reflog.onlyHere")}
                            </span>
                          )}
                        </span>
                        <span className={styles.message}>{entry.message}</span>
                        <span className={styles.more} aria-hidden="true">
                          ⋯
                        </span>
                      </button>
                    </li>
                  ))}
                </ul>
              )}
              {reflog.more && (
                <p className={styles.note}>
                  {t("reflog.more", { count: reflog.entries.length })}
                </p>
              )}
            </>
          )}
        </div>
        <div className={dialogStyles.actions}>
          <button
            ref={closeRef}
            type="button"
            className={dialogStyles.cancel}
            onClick={close}
          >
            {t("reflog.close")}
          </button>
        </div>
      </div>
      {menu && (
        <ContextMenu
          x={menu.x}
          y={menu.y}
          label={t("reflog.entryMenu", { short: menu.entry.short })}
          items={items(menu.entry)}
          onClose={() => setMenu(null)}
        />
      )}
    </div>
  );
}
