import { useState } from "react";
import { useTranslation } from "react-i18next";
import appStyles from "../../app/App.module.css";
import { ContextMenu } from "../../ui/ContextMenu";
import remote from "../remote/remote.module.css";
import { useRepoStore } from "../repo/store";
import { useCreateStore } from "./store";

/** ▾ next to "Open repository": the other ways to get a repository. */
export function NewRepositoryMenu() {
  const { t } = useTranslation();
  const opening = useRepoStore((s) => s.opening);
  const requestInit = useCreateStore((s) => s.requestInit);
  const requestClone = useCreateStore((s) => s.requestClone);
  const [at, setAt] = useState<{ x: number; y: number } | null>(null);
  return (
    <>
      <button
        type="button"
        className={`${appStyles.toolButton} ${remote.menuButton}`}
        aria-label={t("toolbar.newRepository")}
        aria-haspopup="menu"
        disabled={opening}
        onClick={(event) => {
          const rect = event.currentTarget.getBoundingClientRect();
          setAt({ x: rect.left, y: rect.bottom + 2 });
        }}
      >
        ▾
      </button>
      {at && (
        <ContextMenu
          x={at.x}
          y={at.y}
          label={t("toolbar.newRepository")}
          items={[
            {
              label: t("start.init"),
              onSelect: () => void requestInit(t("init.pickerTitle")),
            },
            {
              label: t("start.clone"),
              onSelect: () => void requestClone(),
            },
          ]}
          onClose={() => setAt(null)}
        />
      )}
    </>
  );
}
