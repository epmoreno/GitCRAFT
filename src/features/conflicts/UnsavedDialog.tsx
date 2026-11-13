import { useTranslation } from "react-i18next";
import { closeWindow } from "../../app/windowClose";
import { useRepoStore } from "../repo/store";
import { ConfirmDialog } from "../../ui/ConfirmDialog";
import { useConflictStore } from "./store";

/** Closing a tab or the window would lose unsaved conflict choices. */
export function UnsavedDialog() {
  const { t } = useTranslation();
  const closing = useConflictStore((s) => s.closing);
  const setClosing = useConflictStore((s) => s.setClosing);
  if (!closing) return null;

  async function confirm() {
    if (!closing) return;
    setClosing(null);
    if (closing.kind === "tab") {
      useConflictStore.getState().clearRepo(closing.repoId);
      await useRepoStore.getState().closeTab(closing.repoId, true);
    } else {
      useConflictStore.setState({ drafts: {} });
      await closeWindow();
    }
  }

  return (
    <ConfirmDialog
      title={t("conflicts.unsaved.title")}
      confirmLabel={t(
        closing.kind === "tab"
          ? "conflicts.unsaved.confirmTab"
          : "conflicts.unsaved.confirmWindow",
      )}
      cancelLabel={t("conflicts.cancel")}
      danger
      onConfirm={() => void confirm()}
      onCancel={() => setClosing(null)}
    >
      <p>{t(`conflicts.unsaved.${closing.kind}`)}</p>
      <ul>
        {closing.paths.map((path) => (
          <li key={path}>
            <code>{path}</code>
          </li>
        ))}
      </ul>
    </ConfirmDialog>
  );
}
