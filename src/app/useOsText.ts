import { useTranslation } from "react-i18next";
import type { TOptions } from "i18next";
import i18n from "../i18n";
import { osKey } from "./platform";

/**
 * `t` for texts that depend on the system (Trash or Recycle Bin, where the
 * git configuration lives, where gh keeps the session): uses `key_windows`
 * or `key_linux` when there is one, otherwise `key` (the macOS text).
 */
export function useOsText(): (key: string, options?: TOptions) => string {
  const { t } = useTranslation();
  return (key, options) =>
    t(
      osKey(key, (k) => i18n.exists(k, options)),
      options,
    ) as string;
}
