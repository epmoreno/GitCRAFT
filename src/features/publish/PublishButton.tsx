import { useTranslation } from "react-i18next";
import appStyles from "../../app/App.module.css";
import { Tooltip } from "../../ui/Tooltip";
import { useGraphStore } from "../graph/store";
import { usePublishStore } from "./store";

/** "Publish on GitHub", only for a repository without remotes. */
export function PublishButton({ repoId }: { repoId: number | null }) {
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
  return (
    <Tooltip label={why ?? t("publish.hint")}>
      <button
        type="button"
        className={appStyles.toolButton}
        disabled={why !== null}
        onClick={() => void open(repoId)}
      >
        {t("publish.button")}
      </button>
    </Tooltip>
  );
}
