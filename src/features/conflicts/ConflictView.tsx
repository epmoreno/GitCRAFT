import { useEffect, useRef, useState, type ReactNode } from "react";
import { useTranslation } from "react-i18next";
import { isShortcut } from "../../app/shortcuts";
import { useShortcutLabel } from "../../app/useShortcutLabel";
import {
  commands,
  type CommandError,
  type ConflictBlock,
  type ConflictFile,
  type ConflictSides,
  type ConflictVersion,
  type MarkerLine,
  type Segment,
  type Side,
} from "../../ipc";
import { ConfirmDialog } from "../../ui/ConfirmDialog";
import { ErrorReport } from "../../ui/ErrorReport";
import { Tooltip } from "../../ui/Tooltip";
import { useGraphStore } from "../graph/store";
import { useWorkdirStore } from "../workdir/store";
import {
  blockIndexes,
  blocksOf,
  buildResult,
  chosenText,
  lineCount,
  rawBlock,
  shorten,
  sideNames,
  toTextarea,
  type Choice,
  type Choices,
  type SideNames,
} from "./result";
import { useConflictStore } from "./store";
import styles from "./conflicts.module.css";

type Loaded =
  | { file: ConflictFile; sides: ConflictSides | null; error?: undefined }
  | { error: CommandError; file?: undefined };

type Confirm =
  | { kind: "restart" }
  | { kind: "delete" }
  | { kind: "markers"; lines: MarkerLine[] };

/** Common text up to this many lines is shown; longer runs are folded. */
const SHOWN_COMMON_LINES = 3;

/** Opened from the "Conflicts" section: the two sides of each block, a
 * choice per block and the resulting file, over the history like a diff. */
export function ConflictView({
  repoId,
  path,
}: {
  repoId: number;
  path: string;
}) {
  const { t } = useTranslation();
  const close = useGraphStore((s) => s.closeFile);
  const refresh = useWorkdirStore((s) => s.refresh);
  // Re-read whenever the status changes (a save, an editor, the watcher).
  const version = useWorkdirStore((s) => s.statuses[repoId]?.version ?? 0);
  const [loaded, setLoaded] = useState<Loaded | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<CommandError | null>(null);
  const [confirm, setConfirm] = useState<Confirm | null>(null);

  useEffect(() => {
    let stale = false;
    void Promise.all([
      commands.conflictFile(repoId, path),
      commands.conflictSides(repoId),
    ]).then(([file, sides]) => {
      if (stale) return;
      if (file.status === "error") setLoaded({ error: file.error });
      else
        setLoaded({
          file: file.data,
          sides: sides.status === "ok" ? sides.data : null,
        });
    });
    return () => {
      stale = true;
    };
  }, [repoId, path, version]);

  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (confirm === null && isShortcut("escape", event)) close(repoId);
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [repoId, close, confirm]);

  const file = loaded?.file ?? null;
  const names = sideNames(loaded?.file ? loaded.sides : null, (short) =>
    t("conflicts.revertOf", { short }),
  );

  /** Runs an action; on success refreshes the status. Returns whether it
   * worked. */
  async function run<T>(
    action: () => Promise<
      { status: "ok"; data: T } | { status: "error"; error: CommandError }
    >,
  ): Promise<{ ok: true; data: T } | { ok: false }> {
    setBusy(true);
    setError(null);
    const result = await action();
    setBusy(false);
    if (result.status === "error") {
      setError(result.error);
      return { ok: false };
    }
    void refresh(repoId);
    return { ok: true, data: result.data };
  }

  async function markResolved(force: boolean) {
    const result = await run(() => commands.markResolved(repoId, path, force));
    if (!result.ok) return;
    if (result.data.type === "markersLeft") {
      setConfirm({ kind: "markers", lines: result.data.lines });
      return;
    }
    close(repoId);
  }

  async function keepVersion(side: Side) {
    if (!file) return;
    const chosen = await run(() =>
      commands.chooseVersion(repoId, path, side, file.fingerprint),
    );
    if (chosen.ok) await markResolved(false);
  }

  async function confirmed() {
    const current = confirm;
    setConfirm(null);
    if (!current || !file) return;
    switch (current.kind) {
      case "markers":
        await markResolved(true);
        return;
      case "restart": {
        const done = await run(() =>
          commands.restartConflictFile(repoId, path, file.fingerprint),
        );
        if (done.ok) useConflictStore.getState().clearDraft(repoId, path);
        return;
      }
      case "delete": {
        const done = await run(() =>
          commands.resolveByDeleting(repoId, path, file.fingerprint),
        );
        if (done.ok) close(repoId);
        return;
      }
    }
  }

  return (
    <section
      className={styles.view}
      aria-label={t("conflicts.label", { path })}
    >
      <header className={styles.header}>
        <button
          type="button"
          className={styles.back}
          title={t("diff.backHint")}
          onClick={() => close(repoId)}
        >
          ← {t("diff.back")}
        </button>
        <span className={`${styles.path} selectable`}>{path}</span>
      </header>
      {loaded?.file && loaded.sides?.type === "rebase" && (
        <p className={styles.note} role="note">
          {t("conflicts.rebaseNote", {
            onto: names.ours,
            branch: names.theirs,
          })}
        </p>
      )}
      {error && (
        <ErrorReport
          className={styles.error}
          error={error}
          lead={t("conflicts.failed")}
        />
      )}
      {!loaded ? (
        <p className={styles.message}>{t("diff.loading")}</p>
      ) : loaded.error ? (
        <ErrorReport
          className={styles.message}
          error={loaded.error}
          lead={t("conflicts.loadFailed")}
        />
      ) : loaded.file.content.type === "wholeFile" ? (
        <WholeFile
          file={loaded.file}
          names={names}
          sides={loaded.sides}
          busy={busy}
          onUse={(side) => void keepVersion(side)}
          onDelete={() => setConfirm({ kind: "delete" })}
          onRestart={() => setConfirm({ kind: "restart" })}
        />
      ) : (
        <Blocks
          repoId={repoId}
          file={loaded.file}
          segments={loaded.file.content.segments}
          names={names}
          sides={loaded.sides}
          busy={busy}
          run={run}
          onMark={() => void markResolved(false)}
          onRestart={() => setConfirm({ kind: "restart" })}
        />
      )}
      {confirm && (
        <ConfirmDialog
          title={t(`conflicts.confirm.${confirm.kind}.title`)}
          confirmLabel={t(`conflicts.confirm.${confirm.kind}.confirm`)}
          cancelLabel={t("conflicts.cancel")}
          danger={confirm.kind !== "markers"}
          onConfirm={() => void confirmed()}
          onCancel={() => setConfirm(null)}
        >
          <p>{t(`conflicts.confirm.${confirm.kind}.body`, { path })}</p>
          {confirm.kind === "markers" && (
            <ul className={styles.markerLines}>
              {confirm.lines.map((line) => (
                <li key={line.line}>
                  <span className={styles.lineNo}>
                    {t("conflicts.lineNo", { line: line.line })}
                  </span>
                  <code>{line.text}</code>
                </li>
              ))}
            </ul>
          )}
        </ConfirmDialog>
      )}
    </section>
  );
}

/** A side's name in a button: cut with "…" when long, whole in a tooltip. */
function NameButton({
  label,
  name,
  pressed,
  disabled,
  onClick,
}: {
  label: (name: string) => string;
  name: string;
  pressed?: boolean;
  disabled?: boolean;
  onClick: () => void;
}) {
  const short = shorten(name);
  const button = (
    <button
      type="button"
      className={styles.choice}
      aria-pressed={pressed}
      aria-label={label(name)}
      disabled={disabled}
      onClick={onClick}
    >
      {label(short)}
    </button>
  );
  return short === name ? button : <Tooltip label={name}>{button}</Tooltip>;
}

function sideTitle(
  t: (key: string, options?: Record<string, unknown>) => string,
  sides: ConflictSides | null,
  names: SideNames,
  side: "ours" | "theirs",
): string {
  const type = sides?.type ?? "merge";
  if (side === "ours")
    return t(`conflicts.side.ours.${type}`, { name: names.ours });
  switch (sides?.type) {
    case "rebase":
      return t("conflicts.side.theirs.rebase", {
        name: names.theirs,
        subject: sides.applying?.subject ?? "",
      });
    case "cherryPick":
      return t("conflicts.side.theirs.cherryPick", {
        short: sides.picking.short,
        subject: sides.picking.subject,
      });
    case "revert":
      return t("conflicts.side.theirs.revert", {
        short: sides.reverting.short,
        subject: sides.reverting.subject,
      });
    default:
      return t("conflicts.side.theirs.merge", { name: names.theirs });
  }
}

function Blocks({
  repoId,
  file,
  segments,
  names,
  sides,
  busy,
  run,
  onMark,
  onRestart,
}: {
  repoId: number;
  file: ConflictFile;
  segments: Segment[];
  names: SideNames;
  sides: ConflictSides | null;
  busy: boolean;
  run: <T>(
    action: () => Promise<
      { status: "ok"; data: T } | { status: "error"; error: CommandError }
    >,
  ) => Promise<{ ok: true; data: T } | { ok: false }>;
  onMark: () => void;
  onRestart: () => void;
}) {
  const { t } = useTranslation();
  const label = useShortcutLabel();
  const path = file.path;
  const draft = useConflictStore((s) => s.drafts[repoId]?.[path]);
  const setChoice = useConflictStore((s) => s.setChoice);
  const clearDraft = useConflictStore((s) => s.clearDraft);
  const blocks = blocksOf(segments);
  const stale = draft !== undefined && draft.fingerprint !== file.fingerprint;
  const choices: Choices = draft && !stale ? draft.choices : {};
  const chosen = Object.keys(choices).length;
  const [current, setCurrent] = useState(0);
  const cards = useRef<(HTMLDivElement | null)[]>([]);

  const go = (index: number) => {
    if (blocks.length === 0) return;
    const next = Math.max(0, Math.min(blocks.length - 1, index));
    setCurrent(next);
    const card = cards.current[next];
    card?.scrollIntoView?.({ block: "nearest" });
    card?.focus({ preventScroll: true });
  };

  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      const target = event.target as HTMLElement | null;
      if (target?.tagName === "TEXTAREA") return;
      if (isShortcut("conflictNext", event)) {
        event.preventDefault();
        go(current + 1);
      } else if (isShortcut("conflictPrevious", event)) {
        event.preventDefault();
        go(current - 1);
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  });

  const choose = (index: number, choice: Choice | null) =>
    setChoice(repoId, path, file.fingerprint, index, choice);

  async function save(mark: boolean) {
    if (file.fingerprint === null) return;
    const contents = buildResult(segments, choices);
    const written = await run(() =>
      commands.writeResolution(repoId, path, contents, file.fingerprint ?? ""),
    );
    if (!written.ok) return;
    clearDraft(repoId, path);
    if (mark) onMark();
  }

  const indexes = blockIndexes(segments);
  return (
    <>
      <div className={styles.toolbar}>
        <span className={styles.counter}>
          {blocks.length === 0
            ? t("conflicts.noBlocks")
            : t("conflicts.counter", {
                current: current + 1,
                total: blocks.length,
                chosen,
              })}
        </span>
        <Tooltip
          label={t("conflicts.previous", {
            shortcut: label("conflictPrevious"),
          })}
        >
          <button
            type="button"
            className={styles.nav}
            aria-label={t("conflicts.previous", {
              shortcut: label("conflictPrevious"),
            })}
            disabled={current === 0 || blocks.length === 0}
            onClick={() => go(current - 1)}
          >
            ↑
          </button>
        </Tooltip>
        <Tooltip
          label={t("conflicts.next", { shortcut: label("conflictNext") })}
        >
          <button
            type="button"
            className={styles.nav}
            aria-label={t("conflicts.next", {
              shortcut: label("conflictNext"),
            })}
            disabled={current >= blocks.length - 1}
            onClick={() => go(current + 1)}
          >
            ↓
          </button>
        </Tooltip>
      </div>
      {stale && (
        <div className={styles.stale} role="alert">
          <p>{t("conflicts.staleDraft")}</p>
          <button
            type="button"
            className={styles.choice}
            onClick={() => clearDraft(repoId, path)}
          >
            {t("conflicts.discardDraft")}
          </button>
        </div>
      )}
      <div className={styles.columnsHeader} aria-hidden="true">
        <span title={names.ours}>{sideTitle(t, sides, names, "ours")}</span>
        <span title={names.theirs}>{sideTitle(t, sides, names, "theirs")}</span>
      </div>
      <div className={styles.body}>
        {segments.map((segment, i) => {
          if (segment.type === "common") {
            return <Common key={i} text={segment.text} />;
          }
          const index = indexes[i] ?? 0;
          return (
            <BlockCard
              key={i}
              ref={(el) => {
                cards.current[index] = el;
              }}
              index={index}
              total={blocks.length}
              block={segment.block}
              names={names}
              choice={choices[index] ?? null}
              current={index === current}
              disabled={busy || stale}
              onFocus={() => setCurrent(index)}
              onChoose={(choice) => choose(index, choice)}
            />
          );
        })}
      </div>
      <Result segments={segments} choices={choices} />
      <footer className={styles.footer}>
        <button
          type="button"
          className={styles.secondary}
          disabled={busy}
          onClick={onRestart}
        >
          {t("conflicts.restart")}
        </button>
        <span className={styles.spacer} />
        <button
          type="button"
          className={styles.secondary}
          disabled={busy || stale || chosen === 0}
          onClick={() => void save(false)}
        >
          {t("conflicts.save")}
        </button>
        <button
          type="button"
          className={styles.primary}
          disabled={busy || stale}
          onClick={() => (chosen === 0 ? onMark() : void save(true))}
        >
          {t("conflicts.saveAndMark")}
        </button>
      </footer>
    </>
  );
}

function Common({ text }: { text: string }) {
  const { t } = useTranslation();
  const [open, setOpen] = useState(false);
  const lines = lineCount(text);
  if (lines <= SHOWN_COMMON_LINES || open) {
    return <pre className={styles.common}>{text}</pre>;
  }
  return (
    <button type="button" className={styles.fold} onClick={() => setOpen(true)}>
      ▸ {t("conflicts.sameLines", { count: lines })}
    </button>
  );
}

function BlockCard({
  ref,
  index,
  total,
  block,
  names,
  choice,
  current,
  disabled,
  onFocus,
  onChoose,
}: {
  ref: (el: HTMLDivElement | null) => void;
  index: number;
  total: number;
  block: ConflictBlock;
  names: SideNames;
  choice: Choice | null;
  current: boolean;
  disabled: boolean;
  onFocus: () => void;
  onChoose: (choice: Choice | null) => void;
}) {
  const { t } = useTranslation();
  const [showBase, setShowBase] = useState(false);
  const [editing, setEditing] = useState<string | null>(null);
  const kind = choice?.kind ?? null;
  const use = (name: string) => t("conflicts.use", { name });
  return (
    <div
      ref={ref}
      tabIndex={-1}
      className={styles.block}
      data-current={current || undefined}
      data-chosen={choice ? true : undefined}
      aria-label={t("conflicts.blockLabel", { index: index + 1, total })}
      onFocus={onFocus}
      onClick={onFocus}
    >
      <div className={styles.sides}>
        <pre className={styles.ours} aria-label={names.ours}>
          {block.ours}
        </pre>
        <pre className={styles.theirs} aria-label={names.theirs}>
          {block.theirs}
        </pre>
      </div>
      {showBase && (
        <div className={styles.base}>
          <span className={styles.baseTitle}>{t("conflicts.baseTitle")}</span>
          <pre>{block.base}</pre>
        </div>
      )}
      <div className={styles.choices}>
        <NameButton
          label={use}
          name={names.ours}
          pressed={kind === "ours"}
          disabled={disabled}
          onClick={() => onChoose({ kind: "ours" })}
        />
        <NameButton
          label={use}
          name={names.theirs}
          pressed={kind === "theirs"}
          disabled={disabled}
          onClick={() => onChoose({ kind: "theirs" })}
        />
        <NameButton
          label={(name) => t("conflicts.bothFirst", { name })}
          name={names.ours}
          pressed={kind === "oursFirst"}
          disabled={disabled}
          onClick={() => onChoose({ kind: "oursFirst" })}
        />
        <NameButton
          label={(name) => t("conflicts.bothFirst", { name })}
          name={names.theirs}
          pressed={kind === "theirsFirst"}
          disabled={disabled}
          onClick={() => onChoose({ kind: "theirsFirst" })}
        />
        <button
          type="button"
          className={styles.choice}
          aria-pressed={kind === "edited"}
          disabled={disabled}
          onClick={() =>
            setEditing(
              toTextarea(choice ? chosenText(block, choice) : block.ours),
            )
          }
        >
          {t("conflicts.edit")}
        </button>
        <Tooltip
          label={
            block.base === null
              ? t("conflicts.noBase")
              : t("conflicts.baseHint")
          }
        >
          <button
            type="button"
            className={styles.toggle}
            aria-expanded={showBase}
            disabled={block.base === null}
            onClick={() => setShowBase((v) => !v)}
          >
            {showBase ? t("conflicts.hideBase") : t("conflicts.showBase")}
          </button>
        </Tooltip>
        {choice && (
          <button
            type="button"
            className={styles.toggle}
            disabled={disabled}
            onClick={() => onChoose(null)}
          >
            {t("conflicts.undoChoice")}
          </button>
        )}
      </div>
      {editing !== null && (
        <div className={styles.editor}>
          <textarea
            className={styles.textarea}
            aria-label={t("conflicts.editLabel", { index: index + 1 })}
            value={editing}
            rows={Math.min(16, Math.max(3, lineCount(editing) + 1))}
            onChange={(event) => setEditing(event.target.value)}
          />
          <div className={styles.editorActions}>
            <button
              type="button"
              className={styles.toggle}
              onClick={() => setEditing(null)}
            >
              {t("conflicts.cancel")}
            </button>
            <button
              type="button"
              className={styles.choice}
              onClick={() => {
                onChoose({ kind: "edited", text: editing });
                setEditing(null);
              }}
            >
              {t("conflicts.applyEdit")}
            </button>
          </div>
        </div>
      )}
    </div>
  );
}

/** The file as it will be saved: chosen blocks highlighted, the others
 * still with their markers. */
function Result({
  segments,
  choices,
}: {
  segments: Segment[];
  choices: Choices;
}) {
  const { t } = useTranslation();
  const indexes = blockIndexes(segments);
  const parts: ReactNode[] = segments.map((segment, i) => {
    if (segment.type === "common") return <span key={i}>{segment.text}</span>;
    const index = indexes[i] ?? 0;
    const choice = choices[index];
    return choice ? (
      <mark key={i} className={styles.resolved}>
        {chosenText(segment.block, choice)}
      </mark>
    ) : (
      <span key={i} className={styles.unresolved}>
        <span className={styles.unresolvedLabel}>
          {t("conflicts.unresolvedBlock", { index: index + 1 })}
        </span>
        {rawBlock(segment.block)}
      </span>
    );
  });
  return (
    <div
      className={styles.result}
      role="region"
      aria-label={t("conflicts.result")}
    >
      <h3 className={styles.resultTitle}>{t("conflicts.result")}</h3>
      <pre className={styles.resultText} data-testid="conflict-result">
        {parts}
      </pre>
    </div>
  );
}

function sizeOf(
  version: ConflictVersion | null,
  t: (k: string, o?: Record<string, unknown>) => string,
): string {
  if (!version) return t("conflicts.whole.deleted");
  if (version.kind === "symlink") return t("conflicts.whole.symlink");
  if (version.kind === "submodule") return t("conflicts.whole.submodule");
  return t("conflicts.whole.size", {
    size: new Intl.NumberFormat().format(version.size ?? 0),
  });
}

/** Files that can only be resolved by keeping one version whole. */
function WholeFile({
  file,
  names,
  sides,
  busy,
  onUse,
  onDelete,
  onRestart,
}: {
  file: ConflictFile;
  names: SideNames;
  sides: ConflictSides | null;
  busy: boolean;
  onUse: (side: Side) => void;
  onDelete: () => void;
  onRestart: () => void;
}) {
  const { t } = useTranslation();
  if (file.content.type !== "wholeFile") return null;
  const reason = file.content.reason;
  const use = (name: string) => t("conflicts.use", { name });
  return (
    <div className={styles.whole}>
      <p>
        {reason.type === "malformedMarkers"
          ? t("conflicts.whole.reason.malformedMarkers", { line: reason.line })
          : t(`conflicts.whole.reason.${reason.type}`)}
      </p>
      <dl className={styles.versions}>
        <dt>{sideTitle(t, sides, names, "ours")}</dt>
        <dd>{sizeOf(file.ours, t)}</dd>
        <dt>{sideTitle(t, sides, names, "theirs")}</dt>
        <dd>{sizeOf(file.theirs, t)}</dd>
      </dl>
      <div className={styles.choices}>
        {file.ours && (
          <NameButton
            label={use}
            name={names.ours}
            disabled={busy}
            onClick={() => onUse("ours")}
          />
        )}
        {file.theirs && (
          <NameButton
            label={use}
            name={names.theirs}
            disabled={busy}
            onClick={() => onUse("theirs")}
          />
        )}
        {(!file.ours || !file.theirs) && (
          <button
            type="button"
            className={styles.dangerButton}
            disabled={busy}
            onClick={onDelete}
          >
            {t("conflicts.deleteFile")}
          </button>
        )}
        {reason.type === "malformedMarkers" && (
          <button
            type="button"
            className={styles.secondary}
            disabled={busy}
            onClick={onRestart}
          >
            {t("conflicts.restart")}
          </button>
        )}
      </div>
    </div>
  );
}
