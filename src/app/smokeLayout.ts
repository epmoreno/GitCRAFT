import { useSettingsStore } from "../features/settings/store";

/* Layout checks for the smoke run (src/app/smoke.ts): they need the real
 * web engine's layout, which the unit tests (jsdom) do not have. */

const nextFrame = () =>
  new Promise((resolve) => requestAnimationFrame(() => resolve(null)));

/** Settings → Shortcuts laid out by the real engine: the header stays above
 * the content (nothing scrolls behind it), the close button is a comfortable
 * target and a group title stays at the top while its rows scroll by. */
export async function settingsLayoutProblems(): Promise<string[]> {
  const problems: string[] = [];
  useSettingsStore.getState().openSettings("shortcuts");
  try {
    await nextFrame();
    await nextFrame();
    const dialog = document.querySelector('[role="dialog"]');
    const header = dialog?.querySelector("header");
    const content = header?.nextElementSibling;
    const close = header?.querySelector("button");
    if (!header || !(content instanceof HTMLElement) || !close) {
      return ["settings: the header or its content is missing"];
    }
    const box = close.getBoundingClientRect();
    if (box.width < 28 || box.height < 28) {
      problems.push(`settings: the close button is ${box.width}×${box.height}`);
    }
    const top = content.getBoundingClientRect().top;
    if (top < header.getBoundingClientRect().bottom - 0.5) {
      problems.push("settings: the content starts behind the header");
    }
    const titles = [...content.querySelectorAll("h4")];
    const first = titles[0]?.getBoundingClientRect().top ?? top;
    if (first - top < 4) {
      problems.push("settings: no room above the first shortcut group");
    }
    // Into the middle of the second group: its title must stick at the top.
    const second = titles[1];
    if (second && content.scrollHeight > content.clientHeight) {
      content.scrollTop += second.getBoundingClientRect().top - top + 40;
      await nextFrame();
      const stuck = titles.some(
        (h) => Math.abs(h.getBoundingClientRect().top - top) < 1.5,
      );
      if (!stuck) {
        const at = titles.map((h) =>
          Math.round(h.getBoundingClientRect().top - top),
        );
        problems.push(
          `settings: the group titles do not stick (${at.join(", ")})`,
        );
      }
    }
  } finally {
    useSettingsStore.getState().close();
  }
  return problems;
}
