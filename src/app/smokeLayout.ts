import { createElement } from "react";
import { createRoot } from "react-dom/client";
import { InteractiveRebaseView } from "../features/interactiveRebase/InteractiveRebaseView";
import { useInteractiveRebaseStore } from "../features/interactiveRebase/store";
import type { RangeCommit } from "../ipc";
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

/** A made-up plan for the view's layout check: C combined into B, A kept.
 * Nothing in it reaches git (no action of the view is used). */
function sampleRebaseView(repoId: number) {
  const commit = (label: string, subject: string): RangeCommit => ({
    commit: { oid: label.repeat(40), short: label.repeat(7), subject },
    message: subject,
    author: "GitCRAFT",
    authorTime: Math.floor(Date.now() / 1000) - 3600,
    published: label === "a",
  });
  const commits = [
    commit("a", "Add the guide"),
    commit("b", "Adjust the tooltip"),
    commit("c", `Fix the tooltip ${"and a very long subject ".repeat(8)}`),
  ];
  return {
    repoId,
    from: commits[0]!.commit.oid,
    preview: {
      branch: "main",
      from: commits[0]!.commit.oid,
      base: null,
      refused: null,
      commits,
      publishedIn: ["origin/main"],
      blocked: null,
      blocking: [],
      messages: true,
      exact: true,
      gitVersion: "2.55.0",
      fingerprint: "smoke",
    },
    error: null,
    rows: [
      { oid: commits[2]!.commit.oid, action: "squash" as const, message: null },
      { oid: commits[1]!.commit.oid, action: "pick" as const, message: null },
      { oid: commits[0]!.commit.oid, action: "pick" as const, message: null },
    ],
    combined: {},
    selected: commits[2]!.commit.oid,
    prediction: {
      problem: null,
      dropped: [],
      empty: [],
      stopsAt: null,
      predicted: true,
      commitsAfter: 2,
      rewritten: 2,
      publishedIn: ["origin/main"],
    },
    predicting: false,
    confirming: false,
    changed: null,
  };
}

/** The interactive rebase view (task 13.4) laid out by the real engine:
 * rows of one line and fixed height (dragging measures with it), the line
 * joining a combination drawn, and the buttons inside the view. */
export async function rebaseLayoutProblems(): Promise<string[]> {
  const problems: string[] = [];
  const repoId = -1;
  const host = document.createElement("div");
  host.style.cssText =
    "position:fixed;left:0;top:0;width:820px;height:560px;visibility:hidden";
  document.body.append(host);
  const root = createRoot(host);
  useInteractiveRebaseStore.setState({ view: sampleRebaseView(repoId) });
  try {
    root.render(createElement(InteractiveRebaseView, { repoId }));
    await nextFrame();
    await nextFrame();
    const view = host.querySelector("section");
    const rows = [...host.querySelectorAll<HTMLElement>("[data-rebase-row]")];
    const footer = host.querySelector("footer");
    if (!view || rows.length !== 3 || !footer) {
      return ["rebase view: it did not render"];
    }
    for (const row of rows) {
      const height = row.getBoundingClientRect().height;
      if (Math.abs(height - 34) > 1) {
        problems.push(`rebase view: a row is ${height}px high, not 34`);
      }
      if (row.scrollWidth > row.clientWidth + 1) {
        problems.push("rebase view: a row overflows sideways");
      }
    }
    const lines = rows.map((row) => {
      const chain = row.querySelector('[aria-hidden="true"]:not([data-grip])');
      return chain
        ? parseFloat(getComputedStyle(chain, "::before").height) || 0
        : 0;
    });
    if (!(lines[0]! > 4 && lines[1]! > 4 && lines[2] === 0)) {
      problems.push(`rebase view: the combination line is ${lines.join(", ")}`);
    }
    if (
      footer.getBoundingClientRect().bottom >
      view.getBoundingClientRect().bottom + 0.5
    ) {
      problems.push("rebase view: the buttons are outside the view");
    }
  } finally {
    root.unmount();
    host.remove();
    useInteractiveRebaseStore.setState({ view: null });
  }
  return problems;
}
