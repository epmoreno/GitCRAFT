import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { commands, ghLogin, type CommandError, type GhStatus } from "../../ipc";
import { copyText } from "../../ui/clipboard";
import { ConfirmDialog } from "../../ui/ConfirmDialog";
import { ErrorReport } from "../../ui/ErrorReport";
import styles from "./settings.module.css";

type Login =
  { step: "waiting" } | { step: "code"; code: string; url: string } | null;

/**
 * GitHub account through GitHub CLI (`gh`). GitCRAFT stores no token: gh
 * keeps the session (in the macOS Keychain) and GitCRAFT only asks it who
 * is signed in.
 */
export function GithubSection() {
  const { t } = useTranslation();
  const [status, setStatus] = useState<GhStatus | null>(null);
  const [error, setError] = useState<CommandError | null>(null);
  const [login, setLogin] = useState<Login>(null);
  const [lines, setLines] = useState<string[]>([]);
  const [confirm, setConfirm] = useState<"logout" | "setupGit" | null>(null);
  const [copied, setCopied] = useState(false);
  const [checking, setChecking] = useState(false);

  const show = (
    result:
      | { status: "ok"; data: GhStatus }
      | { status: "error"; error: CommandError },
  ) => {
    if (result.status === "ok") {
      setStatus(result.data);
      setError(null);
    } else if (result.error.kind !== "cancelled") {
      setError(result.error);
    }
  };
  const refresh = () => {
    setChecking(true);
    void commands.ghStatus().then((result) => {
      show(result);
      setChecking(false);
    });
  };

  useEffect(() => {
    let stale = false;
    void commands.ghStatus().then((result) => {
      if (!stale) show(result);
    });
    return () => {
      stale = true;
      void commands.cancelGhLogin();
    };
  }, []);

  const signIn = async () => {
    setLogin({ step: "waiting" });
    setLines([]);
    setCopied(false);
    const result = await ghLogin((event) => {
      if (event.type === "code") {
        setLogin({ step: "code", code: event.code, url: event.url });
      } else if (event.text) {
        setLines((previous) => [...previous, event.text]);
      }
    });
    setLogin(null);
    show(result);
  };

  const account = status?.accounts.find((a) => a.active) ?? status?.accounts[0];

  return (
    <section aria-label={t("settings.section.github")}>
      <h3 className={styles.sectionTitle}>{t("settings.section.github")}</h3>
      <p className={styles.hint}>{t("settings.github.intro")}</p>
      {error && (
        <ErrorReport error={error} lead={t("settings.github.failed")} />
      )}
      {!status && !error && (
        <p className={styles.hint}>{t("settings.github.checking")}</p>
      )}
      {status && status.path === null && (
        <div className={styles.notice} role="status">
          <p>{t("settings.github.notInstalled")}</p>
          <p>
            {t("settings.github.install")}{" "}
            <code className="selectable">brew install gh</code>
          </p>
          <p className={styles.hint}>{t("settings.github.installOther")}</p>
          <button type="button" className={styles.button} onClick={refresh}>
            {t("settings.github.checkAgain")}
          </button>
        </div>
      )}
      {status?.path && (
        <>
          <p className={styles.hint}>
            {t("settings.github.found", {
              path: status.path,
              version: status.version ?? "gh",
            })}
          </p>
          {login ? (
            <div className={styles.notice} role="status" aria-live="polite">
              {login.step === "waiting" ? (
                <p>{t("settings.github.starting")}</p>
              ) : (
                <>
                  <p>{t("settings.github.typeCode", { url: login.url })}</p>
                  <p
                    className={`${styles.code} selectable`}
                    aria-label={t("settings.github.code")}
                  >
                    {login.code}
                  </p>
                  <div className={styles.fieldControl}>
                    <button
                      type="button"
                      className={styles.button}
                      onClick={async () =>
                        setCopied(await copyText(login.code))
                      }
                    >
                      {copied
                        ? t("settings.github.copied")
                        : t("settings.github.copy")}
                    </button>
                  </div>
                  <p className={styles.hint}>{t("settings.github.waiting")}</p>
                </>
              )}
              {login.step === "waiting" && lines.length > 0 && (
                <pre className={`${styles.output} selectable`}>
                  {lines.join("\n")}
                </pre>
              )}
              <button
                type="button"
                className={styles.button}
                onClick={() => void commands.cancelGhLogin()}
              >
                {t("settings.github.cancel")}
              </button>
            </div>
          ) : status.sessionUnreachable ? (
            <div className={styles.notice} role="status">
              <p>{t("errors.ghUnreachable")}</p>
              <button
                type="button"
                className={styles.button}
                disabled={checking}
                onClick={refresh}
              >
                {t("settings.github.checkAgain")}
              </button>
            </div>
          ) : account ? (
            <div className={styles.field}>
              <p>
                {t("settings.github.signedIn")}{" "}
                <strong className="selectable">@{account.login}</strong>
              </p>
              {!account.valid && (
                <p className={styles.warning}>{t("settings.github.expired")}</p>
              )}
              <div className={styles.fieldControl}>
                {!account.valid && (
                  <button
                    type="button"
                    className={styles.primary}
                    onClick={() => void signIn()}
                  >
                    {t("settings.github.signIn")}
                  </button>
                )}
                <button
                  type="button"
                  className={styles.button}
                  onClick={() => setConfirm("logout")}
                >
                  {t("settings.github.signOut")}
                </button>
              </div>
            </div>
          ) : (
            <div className={styles.field}>
              <p>{t("settings.github.noSession")}</p>
              <div className={styles.fieldControl}>
                <button
                  type="button"
                  className={styles.primary}
                  onClick={() => void signIn()}
                >
                  {t("settings.github.signIn")}
                </button>
              </div>
            </div>
          )}

          <div
            className={styles.field}
            role="group"
            aria-label={t("settings.github.gitTitle")}
          >
            <div className={styles.fieldLabel}>
              {t("settings.github.gitTitle")}
            </div>
            {status.gitUsesGh ? (
              <p>{t("settings.github.gitUsesGh")}</p>
            ) : (
              <>
                <p>
                  {status.generalHelpers.length > 0
                    ? t("settings.github.gitUsesHelper", {
                        helper: status.generalHelpers.join(", "),
                      })
                    : t("settings.github.gitNoHelper")}
                </p>
                <div className={styles.fieldControl}>
                  <button
                    type="button"
                    className={styles.button}
                    onClick={() => setConfirm("setupGit")}
                  >
                    {t("settings.github.useGh")}
                  </button>
                </div>
              </>
            )}
          </div>
        </>
      )}

      {confirm === "logout" && account && (
        <ConfirmDialog
          title={t("settings.github.signOutTitle", { login: account.login })}
          confirmLabel={t("settings.github.signOut")}
          cancelLabel={t("settings.github.cancel")}
          onCancel={() => setConfirm(null)}
          onConfirm={async () => {
            setConfirm(null);
            show(await commands.ghLogout(account.login));
          }}
        >
          <p>{t("settings.github.signOutBody", { login: account.login })}</p>
        </ConfirmDialog>
      )}
      {confirm === "setupGit" && status?.path && (
        <ConfirmDialog
          title={t("settings.github.useGhTitle")}
          confirmLabel={t("settings.github.useGhConfirm")}
          cancelLabel={t("settings.github.cancel")}
          onCancel={() => setConfirm(null)}
          onConfirm={async () => {
            setConfirm(null);
            show(await commands.ghSetupGit());
          }}
        >
          <p>{t("settings.github.useGhBody")}</p>
          <pre className={`${styles.output} selectable`}>
            {[
              '[credential "https://github.com"]',
              "\thelper =",
              `\thelper = !${status.path} auth git-credential`,
              '[credential "https://gist.github.com"]',
              "\thelper =",
              `\thelper = !${status.path} auth git-credential`,
            ].join("\n")}
          </pre>
          <p>
            {t("settings.github.useGhNow", {
              helper:
                status.githubHelpers.length > 0
                  ? status.githubHelpers.join(", ")
                  : status.generalHelpers.join(", ") ||
                    t("settings.git.notSet"),
            })}
          </p>
          <p className={styles.hint}>{t("settings.github.useGhUndo")}</p>
        </ConfirmDialog>
      )}
    </section>
  );
}
