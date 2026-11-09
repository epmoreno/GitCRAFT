import { os } from "../../app/platform";
import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import type { TFunction } from "i18next";
import type { CommandOutcome, CommandRecord } from "../../ipc";
import { useRemoteStore } from "../remote/store";
import { useActiveRepository } from "../repo/store";
import styles from "./operations.module.css";
import { ranIn, useOperationsStore } from "./store";

/** "120 ms", "1.4 s", "2 min 5 s", in the UI language. */
export function formatDuration(ms: number, language: string): string {
  const unit = (value: number, unit: string, digits = 0) =>
    new Intl.NumberFormat(language, {
      style: "unit",
      unit,
      unitDisplay: "narrow",
      maximumFractionDigits: digits,
    }).format(value);
  if (ms < 1000) return unit(Math.round(ms), "millisecond");
  if (ms < 60_000) return unit(ms / 1000, "second", 1);
  const minutes = Math.floor(ms / 60_000);
  const seconds = Math.round((ms % 60_000) / 1000);
  return `${unit(minutes, "minute")} ${unit(seconds, "second")}`;
}

function outcomeText(t: TFunction, outcome: CommandOutcome): string {
  switch (outcome.type) {
    case "succeeded":
      return t("operations.outcome.succeeded");
    case "failed":
      return outcome.code === null || outcome.code === undefined
        ? t("operations.outcome.killed")
        : t("operations.outcome.failed", { code: outcome.code });
    case "cancelled":
      return t("operations.outcome.cancelled");
    case "timedOut":
      return t("operations.outcome.timedOut");
    case "error":
      return t("operations.outcome.error");
  }
}

/** Last folder of a path, to tell repositories apart. Windows paths use
 * "\\" (elsewhere it is a valid character in a folder name). */
export function folderName(path: string) {
  const separator = os() === "windows" ? /[\\/]/ : /\//;
  return path.split(separator).filter(Boolean).pop() || path;
}

/** Operations in progress (with progress and cancel) and the history of
 * git commands: exact command line, duration and outcome. */
export function OperationsPanel() {
  const { t, i18n } = useTranslation();
  const open = useOperationsStore((s) => s.open);
  const records = useOperationsStore((s) => s.records);
  const onlyActive = useOperationsStore((s) => s.onlyActive);
  const hideReads = useOperationsStore((s) => s.hideReads);
  const setOnlyActive = useOperationsStore((s) => s.setOnlyActive);
  const setHideReads = useOperationsStore((s) => s.setHideReads);
  const clear = useOperationsStore((s) => s.clear);
  const close = useOperationsStore((s) => s.close);
  const remote = useRemoteStore((s) => s.running);
  const cancelRemote = useRemoteStore((s) => s.cancel);
  const active = useActiveRepository();
  const [now, setNow] = useState(() => Date.now());

  const filterByTab = onlyActive && active !== null;
  const shown = records.filter(
    (r) =>
      (!filterByTab || ranIn(r, active.info.path)) && (!hideReads || r.write),
  );
  const running = shown.filter((r) => !r.outcome);
  const finished = shown.filter((r) => r.outcome).reverse();
  const remoteShown =
    remote !== null && (!filterByTab || remote.repoId === active.id);

  const ticking = open && running.length > 0;
  useEffect(() => {
    if (!ticking) return;
    const timer = window.setInterval(() => setNow(Date.now()), 1000);
    return () => window.clearInterval(timer);
  }, [ticking]);

  if (!open) return null;
  const language = i18n.language;

  return (
    <section className={styles.panel} aria-label={t("operations.title")}>
      <header className={styles.header}>
        <h2 className={styles.title}>{t("operations.title")}</h2>
        <label className={styles.option}>
          <input
            type="checkbox"
            checked={onlyActive}
            disabled={active === null}
            onChange={(e) => setOnlyActive(e.target.checked)}
          />
          {t("operations.onlyActive")}
        </label>
        <label className={styles.option}>
          <input
            type="checkbox"
            checked={hideReads}
            onChange={(e) => setHideReads(e.target.checked)}
          />
          {t("operations.hideReads")}
        </label>
        <span className={styles.spacer} />
        <button
          type="button"
          className={styles.button}
          disabled={!records.some((r) => r.outcome)}
          onClick={() => void clear()}
        >
          {t("operations.clear")}
        </button>
        <button
          type="button"
          className={styles.button}
          aria-label={t("operations.close")}
          onClick={close}
        >
          ×
        </button>
      </header>

      <div className={styles.body}>
        <h3 className={styles.section}>{t("operations.running")}</h3>
        {!remoteShown && running.length === 0 && (
          <p className={styles.empty}>{t("operations.nothingRunning")}</p>
        )}
        {remoteShown && (
          <div className={styles.remote} role="status">
            <span>
              {t(`remote.running.${remote.kind}`, {
                remote: remote.remote ?? t("remote.allRemotes"),
              })}
            </span>
            <progress
              className={styles.bar}
              max={100}
              value={remote.percent ?? undefined}
              aria-label={t("remote.progress")}
            />
            <button
              type="button"
              className={styles.button}
              onClick={() => void cancelRemote()}
            >
              {t("remote.cancel")}
            </button>
          </div>
        )}
        {running.length > 0 && (
          <ul className={styles.list} aria-label={t("operations.running")}>
            {running.map((r) => (
              <Row
                key={r.id}
                record={r}
                folder={filterByTab ? null : folderName(r.cwd)}
                time={new Date(r.startedAt).toLocaleTimeString(language)}
                duration={formatDuration(
                  Math.max(0, now - r.startedAt),
                  language,
                )}
                result={t("operations.outcome.running")}
              />
            ))}
          </ul>
        )}

        <h3 className={styles.section}>{t("operations.history")}</h3>
        {finished.length === 0 ? (
          <p className={styles.empty}>{t("operations.noHistory")}</p>
        ) : (
          <ul className={styles.list} aria-label={t("operations.history")}>
            {finished.map((r) => (
              <Row
                key={r.id}
                record={r}
                folder={filterByTab ? null : folderName(r.cwd)}
                time={new Date(r.startedAt).toLocaleTimeString(language)}
                duration={formatDuration(r.durationMs ?? 0, language)}
                result={r.outcome ? outcomeText(t, r.outcome) : ""}
              />
            ))}
          </ul>
        )}
      </div>
    </section>
  );
}

function Row({
  record,
  folder,
  time,
  duration,
  result,
}: {
  record: CommandRecord;
  folder: string | null;
  time: string;
  duration: string;
  result: string;
}) {
  const { t } = useTranslation();
  const failed = record.outcome && record.outcome.type !== "succeeded";
  return (
    <li className={styles.row} title={record.cwd}>
      <span className={styles.time}>{time}</span>
      {folder !== null && <span className={styles.folder}>{folder}</span>}
      <code className={`${styles.command} selectable`}>{record.command}</code>
      {record.write && (
        <span className={styles.write}>{t("operations.write")}</span>
      )}
      <span className={styles.duration}>{duration}</span>
      <span
        className={
          !record.outcome
            ? styles.duration
            : failed
              ? styles.failed
              : styles.result
        }
      >
        {result}
      </span>
    </li>
  );
}
