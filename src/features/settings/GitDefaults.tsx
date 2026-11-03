import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import {
  commands,
  type CommandError,
  type ConfigChange,
  type GlobalDefaults,
  type PullStrategy,
} from "../../ipc";
import { ErrorReport } from "../../ui/ErrorReport";
import { ConfigConfirm, type ConfigRow } from "./ConfigConfirm";
import { Field } from "./SettingsDialog";
import styles from "./settings.module.css";

/** A change to git's global config waiting for confirmation. */
type Pending = {
  /** Each key, with its value now and the one it would get. */
  rows: ConfigRow[];
  run: () => Promise<
    | { status: "ok"; data: GlobalDefaults }
    | { status: "error"; error: CommandError }
  >;
};

const STRATEGIES: (PullStrategy | null)[] = [
  null,
  "merge",
  "rebase",
  "fastForwardOnly",
];

/**
 * Default branch and pull strategy: they live in git's global config, so
 * they also apply in the terminal. Every change shows what it writes, with
 * the value it replaces, before doing it.
 */
export function GitDefaults() {
  const { t } = useTranslation();
  const [defaults, setDefaults] = useState<GlobalDefaults | null>(null);
  const [error, setError] = useState<CommandError | null>(null);
  const [branch, setBranch] = useState("");
  const [pending, setPending] = useState<Pending | null>(null);

  useEffect(() => {
    void commands.gitDefaults().then((result) => {
      if (result.status === "ok") {
        setDefaults(result.data);
        setBranch(result.data.defaultBranch ?? "");
      } else {
        setError(result.error);
      }
    });
  }, []);

  if (!defaults) {
    return error ? (
      <ErrorReport error={error} lead={t("settings.git.failed")} />
    ) : null;
  }

  const current = (key: string) =>
    key === "init.defaultBranch"
      ? defaults.defaultBranch
      : key === "pull.rebase"
        ? defaults.pullRebase
        : key === "pull.ff"
          ? defaults.pullFf
          : null;

  const confirmBranch = (name: string | null) =>
    setPending({
      rows: [
        {
          key: "init.defaultBranch",
          before: defaults.defaultBranch,
          after: name,
        },
      ],
      run: () => commands.setDefaultBranch(name),
    });

  const confirmStrategy = async (strategy: PullStrategy | null) => {
    const changes = await commands.pullStrategyChanges(strategy);
    if (changes.status !== "ok") return setError(changes.error);
    setPending({
      rows: changes.data.map((c: ConfigChange) => ({
        key: c.key,
        before: current(c.key),
        after: c.value,
      })),
      run: () => commands.setDefaultPullStrategy(strategy),
    });
  };

  const trimmed = branch.trim();
  return (
    <>
      <h3 className={styles.subTitle}>{t("settings.git.title")}</h3>
      <p className={styles.hint}>{t("settings.git.globalNote")}</p>
      {error && <ErrorReport error={error} lead={t("settings.git.failed")} />}
      <Field
        label={t("settings.git.defaultBranch")}
        hint={
          defaults.defaultBranch === null
            ? t("settings.git.defaultBranchUnset")
            : undefined
        }
      >
        <input
          className={styles.input}
          aria-label={t("settings.git.defaultBranch")}
          value={branch}
          placeholder="main"
          spellCheck={false}
          onChange={(event) => setBranch(event.target.value)}
        />
        <button
          type="button"
          className={styles.button}
          disabled={trimmed === "" || trimmed === defaults.defaultBranch}
          onClick={() => confirmBranch(trimmed)}
        >
          {t("settings.git.save")}
        </button>
        {defaults.defaultBranch !== null && (
          <button
            type="button"
            className={styles.button}
            onClick={() => confirmBranch(null)}
          >
            {t("settings.git.unset")}
          </button>
        )}
      </Field>
      <Field label={t("settings.git.pull")} hint={t("settings.git.pullHint")}>
        <div className={styles.choices}>
          {STRATEGIES.map((strategy) => (
            <label key={strategy ?? "unset"} className={styles.choice}>
              <input
                type="radio"
                name="pull-strategy"
                checked={defaults.pullStrategy === strategy}
                onChange={() => void confirmStrategy(strategy)}
              />
              {t(`settings.git.strategy.${strategy ?? "unset"}`)}
            </label>
          ))}
        </div>
      </Field>
      {pending && (
        <ConfigConfirm
          scope="global"
          rows={pending.rows}
          onCancel={() => setPending(null)}
          onConfirm={async () => {
            const result = await pending.run();
            setPending(null);
            if (result.status === "ok") {
              setDefaults(result.data);
              setBranch(result.data.defaultBranch ?? "");
              setError(null);
            } else {
              setError(result.error);
            }
          }}
        />
      )}
    </>
  );
}
