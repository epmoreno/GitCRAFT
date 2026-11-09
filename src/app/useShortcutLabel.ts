import { useTranslation } from "react-i18next";
import { shortcutLabel, type ShortcutId } from "./shortcuts";

/** Translates the keys that are written with their name (Space, Esc…). */
export function useKeyNames(): (key: string) => string {
  const { t } = useTranslation();
  return (key) => t(`settings.shortcuts.key.${key}`);
}

/** How shortcut `id` is written on this system, for hints and tooltips. */
export function useShortcutLabel(): (id: ShortcutId, key?: string) => string {
  const named = useKeyNames();
  return (id, key) => shortcutLabel(id, named, key);
}
