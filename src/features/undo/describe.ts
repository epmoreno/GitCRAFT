import type { TFunction } from "i18next";
import type { OpKind } from "../../ipc";

/** "el commit «x»", "el merge de «feature»"…: what an operation was, to
 * fit in "¿Deshacer …?", "Deshacer … (⌘Z)" and "Deshecho: …". */
export function describeOp(t: TFunction, kind: OpKind): string {
  switch (kind.type) {
    case "commit":
    case "amend":
    case "cherryPick":
    case "revert":
      return t(`undo.what.${kind.type}`, { summary: kind.summary });
    case "merge":
      return t(kind.fastForward ? "undo.what.fastForward" : "undo.what.merge", {
        source: kind.source,
      });
    case "rebase":
      return t("undo.what.rebase", { onto: kind.onto });
    case "reset":
      return t("undo.what.reset", { target: kind.target.slice(0, 7) });
    case "pull":
    case "checkout":
      return t(`undo.what.${kind.type}`, { branch: kind.branch });
    case "createBranch":
    case "deleteBranch":
    case "createTag":
    case "deleteTag":
      return t(`undo.what.${kind.type}`, { name: kind.name });
    case "renameBranch":
      return t("undo.what.renameBranch", { old: kind.old, new: kind.new });
  }
}
