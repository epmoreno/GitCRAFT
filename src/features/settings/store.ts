import { create } from "zustand";
import i18n, { detectLanguage } from "../../i18n";
import {
  commands,
  setUiZoom,
  type CommandError,
  type GeneralSettings,
  type TextSize,
} from "../../ipc";

export type SettingsSection = "general" | "identity" | "github" | "shortcuts";

/** Zoom of the whole UI for each text size. */
export const TEXT_ZOOM: Record<TextSize, number> = {
  small: 0.9,
  normal: 1,
  large: 1.15,
  larger: 1.3,
};

type SettingsState = {
  open: boolean;
  section: SettingsSection;
  general: GeneralSettings;
  /** Reading or saving the settings failed. */
  error: CommandError | null;
  openSettings: (section?: SettingsSection) => void;
  showSection: (section: SettingsSection) => void;
  close: () => void;
  /** Reads the saved settings and applies them (language, text size). */
  load: () => Promise<void>;
  updateGeneral: (patch: Partial<GeneralSettings>) => Promise<void>;
};

export const initialSettingsState = {
  open: false,
  section: "general" as SettingsSection,
  general: {
    language: null,
    textSize: "normal",
    cloneFolder: null,
    autoFetchMinutes: null,
  } satisfies GeneralSettings,
  error: null,
};

function applyLanguage(language: GeneralSettings["language"]) {
  void i18n.changeLanguage(
    language ?? detectLanguage(globalThis.navigator?.language),
  );
}

function applyTextSize(size: TextSize) {
  void setUiZoom(TEXT_ZOOM[size]).catch((error: unknown) => {
    console.warn("could not set the text size", error);
  });
}

export const useSettingsStore = create<SettingsState>()((set, get) => ({
  ...initialSettingsState,

  openSettings: (section) =>
    set((s) => ({ open: true, section: section ?? s.section })),
  showSection: (section) => set({ section }),
  close: () => set({ open: false }),

  load: async () => {
    const result = await commands.generalSettings();
    if (result.status === "ok") {
      set({ general: result.data, error: null });
      // The system language is already in place (i18n detects it at start).
      if (result.data.language !== null) applyLanguage(result.data.language);
      applyTextSize(result.data.textSize);
    } else {
      set({ error: result.error });
    }
  },

  updateGeneral: async (patch) => {
    const general = { ...get().general, ...patch };
    set({ general });
    if ("language" in patch) applyLanguage(general.language);
    if (patch.textSize) applyTextSize(general.textSize);
    const result = await commands.setGeneralSettings(general);
    set({ error: result.status === "ok" ? null : result.error });
  },
}));
