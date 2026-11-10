import { useOsText } from "../../app/useOsText";
import { useCallback, useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import {
  commands,
  type CommandError,
  type EffectiveValue,
  type Identity,
  type IdentityInfo,
} from "../../ipc";
import { ErrorReport } from "../../ui/ErrorReport";
import { useActiveRepository } from "../repo/store";
import { ConfigConfirm, type ConfigRow } from "./ConfigConfirm";
import { Field } from "./SettingsDialog";
import styles from "./settings.module.css";

/** Basic shape only: git accepts anything, this catches typos. */
export const plausibleEmail = (email: string) =>
  email.trim() === "" || /^[^\s@]+@[^\s@]+$/.test(email.trim());

/** Git's identity: global, and for the active tab the repository's own. */
export function IdentitySection() {
  const { t } = useTranslation();
  const osT = useOsText();
  const repo = useActiveRepository();
  const repoId = repo?.id ?? null;
  const [info, setInfo] = useState<IdentityInfo | null>(null);
  const [error, setError] = useState<CommandError | null>(null);

  const load = useCallback(
    () =>
      commands.gitIdentity(repoId).then((result) => {
        if (result.status === "ok") setInfo(result.data);
        else setError(result.error);
      }),
    [repoId],
  );
  useEffect(() => {
    let stale = false;
    void commands.gitIdentity(repoId).then((result) => {
      if (stale) return;
      if (result.status === "ok") setInfo(result.data);
      else setError(result.error);
    });
    return () => {
      stale = true;
    };
  }, [repoId]);

  return (
    <section aria-label={t("settings.section.identity")}>
      <h3 className={styles.sectionTitle}>{t("settings.section.identity")}</h3>
      <p className={styles.hint}>{t("settings.identity.intro")}</p>
      {error && <ErrorReport error={error} lead={t("settings.git.failed")} />}
      {info && (
        <>
          <IdentityForm
            key={`global:${JSON.stringify(info.global)}`}
            title={t("settings.identity.global")}
            hint={osT("settings.identity.globalHint")}
            scope="global"
            current={info.global}
            onSaved={() => {
              setError(null);
              void load();
            }}
            onError={setError}
            repoId={null}
          />
          {repo && info.repo && (
            <>
              <IdentityForm
                key={`repo:${repo.id}:${JSON.stringify(info.repo)}`}
                title={t("settings.identity.repo", { name: repo.info.name })}
                hint={t("settings.identity.repoHint")}
                scope="local"
                current={info.repo}
                inherited={info.global}
                onSaved={() => {
                  setError(null);
                  void load();
                }}
                onError={setError}
                repoId={repo.id}
              />
              <p className={styles.effective}>
                {t("settings.identity.effective", { name: repo.info.name })}{" "}
                <strong className="selectable">
                  {effectiveText(info.effectiveName, info.effectiveEmail, t)}
                </strong>
              </p>
              <ul className={styles.origins}>
                <Origin
                  label={t("settings.identity.name")}
                  value={info.effectiveName}
                />
                <Origin
                  label={t("settings.identity.email")}
                  value={info.effectiveEmail}
                />
              </ul>
            </>
          )}
        </>
      )}
    </section>
  );
}

function effectiveText(
  name: EffectiveValue | null,
  email: EffectiveValue | null,
  t: ReturnType<typeof useTranslation>["t"],
) {
  if (!name && !email) return t("settings.identity.none");
  return `${name?.value ?? "?"} <${email?.value ?? "?"}>`;
}

function Origin({
  label,
  value,
}: {
  label: string;
  value: EffectiveValue | null;
}) {
  const { t } = useTranslation();
  const where = !value
    ? t("settings.identity.from.none")
    : value.scope === "local"
      ? t("settings.identity.from.local")
      : value.scope === "global"
        ? t("settings.identity.from.global")
        : t("settings.identity.from.other", {
            scope: value.scope,
            origin: value.origin.replace(/^file:/, ""),
          });
  return (
    <li className={styles.hint}>
      {label}: {where}
    </li>
  );
}

function IdentityForm({
  title,
  hint,
  scope,
  current,
  inherited,
  repoId,
  onSaved,
  onError,
}: {
  title: string;
  hint: string;
  scope: "global" | "local";
  current: Identity;
  /** In a repository, the global values used when its own are empty. */
  inherited?: Identity;
  repoId: number | null;
  onSaved: () => void;
  onError: (error: CommandError) => void;
}) {
  const { t } = useTranslation();
  const [name, setName] = useState(current.name ?? "");
  const [email, setEmail] = useState(current.email ?? "");
  const [pending, setPending] = useState<{
    identity: Identity;
    rows: ConfigRow[];
  } | null>(null);
  const changed =
    name.trim() !== (current.name ?? "") ||
    email.trim() !== (current.email ?? "");
  const emailOk = plausibleEmail(email);

  const ask = async (identity: Identity) => {
    const result = await commands.identityChanges(identity);
    if (result.status !== "ok") return onError(result.error);
    const before = (key: string) =>
      key === "user.name" ? current.name : current.email;
    setPending({
      identity,
      rows: result.data
        .filter((c) => c.value !== before(c.key))
        .map((c) => ({ key: c.key, before: before(c.key), after: c.value })),
    });
  };

  return (
    <div className={styles.identity} role="group" aria-label={title}>
      <h4 className={styles.subTitle}>{title}</h4>
      <p className={styles.hint}>{hint}</p>
      <Field label={t("settings.identity.name")}>
        <input
          className={styles.input}
          aria-label={t("settings.identity.name")}
          value={name}
          placeholder={inherited?.name ?? ""}
          spellCheck={false}
          onChange={(event) => setName(event.target.value)}
        />
      </Field>
      <Field
        label={t("settings.identity.email")}
        hint={emailOk ? undefined : t("settings.identity.badEmail")}
      >
        <input
          className={styles.input}
          aria-label={t("settings.identity.email")}
          aria-invalid={!emailOk}
          type="email"
          value={email}
          placeholder={inherited?.email ?? ""}
          spellCheck={false}
          onChange={(event) => setEmail(event.target.value)}
        />
      </Field>
      <div className={styles.fieldControl}>
        <button
          type="button"
          className={styles.button}
          disabled={!changed || !emailOk}
          onClick={() => void ask({ name, email })}
        >
          {t("settings.git.save")}
        </button>
        {scope === "local" &&
          (current.name !== null || current.email !== null) && (
            <button
              type="button"
              className={styles.button}
              onClick={() => void ask({ name: null, email: null })}
            >
              {t("settings.identity.useGlobal")}
            </button>
          )}
      </div>
      {pending && (
        <ConfigConfirm
          scope={scope}
          rows={pending.rows}
          onCancel={() => setPending(null)}
          onConfirm={async () => {
            const result = await commands.setGitIdentity(
              repoId,
              pending.identity,
            );
            setPending(null);
            if (result.status === "ok") onSaved();
            else onError(result.error);
          }}
        />
      )}
    </div>
  );
}
