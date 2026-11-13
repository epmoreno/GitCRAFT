import { useTranslation } from "react-i18next";
import { confirmClosing } from "../../app/quit";
import { ConfirmDialog } from "../../ui/ConfirmDialog";
import { useConflictStore } from "./store";

/** Closing a tab or the window would lose unsaved conflict choices. */
export function UnsavedDialog() {
  const { t } = useTranslation();
  const closing = useConflictStore((s) => s.closing);
  const setClosing = useConflictStore((s) => s.setClosing);
  if (!closing) return null;

  return (
    <ConfirmDialog
      title={t("conflicts.unsaved.title")}
      confirmLabel={t(
        {
          tab: "conflicts.unsaved.confirmTab",
          window: "conflicts.unsaved.confirmWindow",
          quit: "conflicts.unsaved.confirmQuit",
        }[closing.kind],
      )}
      cancelLabel={t("conflicts.cancel")}
      danger
      onConfirm={() => void confirmClosing()}
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
