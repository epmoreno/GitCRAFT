import { useTranslation } from "react-i18next";
import appStyles from "../../app/App.module.css";
import { Tooltip } from "../../ui/Tooltip";
import { useGraphStore } from "../graph/store";
import { usePublishStore } from "./store";

/** "Publish on GitHub", only for a repository without remotes: whether (and
 * why not) and what it does. `null`: nothing to publish. */
export function usePublishAction(repoId: number | null) {
  const { t } = useTranslation();
  const refs = useGraphStore((s) =>
    repoId === null ? null : (s.graphs[repoId]?.refs ?? null),
  );
  const open = usePublishStore((s) => s.open);
  if (repoId === null || !refs || refs.remotes.length > 0) return null;
  const why =
    refs.head.type === "detached"
      ? t("publish.needsBranch")
      : !refs.head.oid
        ? t("publish.needsCommit")
        : null;
  return { why, run: () => void open(repoId) };
}

export function PublishButton({ repoId }: { repoId: number | null }) {
  const { t } = useTranslation();
  const action = usePublishAction(repoId);
  if (!action) return null;
  return (
    <Tooltip label={action.why ?? t("publish.hint")}>
      <button
        type="button"
        className={appStyles.toolButton}
        disabled={action.why !== null}
        onClick={action.run}
      >
        {t("publish.button")}
      </button>
    </Tooltip>
  );
}
