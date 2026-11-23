import { createElement } from "react";
import { createRoot } from "react-dom/client";
import { InteractiveRebaseView } from "../features/interactiveRebase/InteractiveRebaseView";
import { useInteractiveRebaseStore } from "../features/interactiveRebase/store";
import type { RangeCommit } from "../ipc";
import { useSettingsStore } from "../features/settings/store";
import { frameOrTick, settles } from "./settle";

/* Layout checks for the smoke run (src/app/smoke.ts): they need the real
 * web engine's layout, which the unit tests (jsdom) do not have. */

/** The web engine's version (WebView2 on Windows reports "Edg/…"), for the
 * log: a layout that changes with the engine shows here. */
function engineVersion(): string {
  const ua = navigator.userAgent;
  const found = ua.match(/(Edg|Chrome|Version|AppleWebKit|WebKitGTK)\/[\d.]+/g);
  return found ? found.join(" ") : ua;
}

/** What a layout check found: `problems` fail the run; `notes` are measures
 * the log keeps even when everything is fine. */
export interface LayoutReport {
  problems: string[];
  notes: string[];
}

/** Settings → Shortcuts laid out by the real engine: the header stays above
 * the content (nothing scrolls behind it), the close button is a comfortable
 * target and a group title stays at the top while its rows scroll by. */
export async function settingsLayout(): Promise<LayoutReport> {
  const problems: string[] = [];
  const notes: string[] = [];
  useSettingsStore.getState().openSettings("shortcuts");
  try {
    await frameOrTick();
    await frameOrTick();
    const findHeader = () =>
      document.querySelector('[role="dialog"]')?.querySelector("header");
    const opened = await settles(
      () =>
        !!findHeader()?.querySelector("button") &&
        (findHeader()?.nextElementSibling?.querySelectorAll("h4").length ?? 0) >
          1,
    );
    const header = findHeader();
    const content = header?.nextElementSibling;
    const close = header?.querySelector("button");
    if (!opened.ok || !header || !(content instanceof HTMLElement) || !close) {
      problems.push("settings: the header or its content is missing");
      return { problems, notes };
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
      // From the content's top as it is at each moment: the dialog is
      // centred, so it moves if the window changes size meanwhile.
      const fromTop = (title: Element) =>
        title.getBoundingClientRect().top - content.getBoundingClientRect().top;
      const measure = () =>
        `titles (${titles.map((h) => Math.round(fromTop(h))).join(", ")}), ` +
        `content top ${content.getBoundingClientRect().top}, ` +
        `header bottom ${header.getBoundingClientRect().bottom}, ` +
        `scrolled ${content.scrollTop} ` +
        `(max ${content.scrollHeight - content.clientHeight}), ` +
        `window ${window.innerWidth}×${window.innerHeight}`;
      const before = measure();
      const asked = content.scrollTop + fromTop(second) + 40;
      content.scrollTop = asked;
      const came = await frameOrTick();
      const firstFrame = measure();
      const stuck = await settles(() =>
        titles.some((h) => Math.abs(fromTop(h)) < 1.5),
      );
      const style = getComputedStyle(second);
      notes.push(
        `settings titles: asked to scroll to ${asked}, ` +
          `title position ${style.position} top ${style.top}, ` +
          `content padding-top ${getComputedStyle(content).paddingTop}, ` +
          `device pixel ratio ${window.devicePixelRatio}, ` +
          `engine ${engineVersion()}`,
        `settings titles, before scrolling: ${before}`,
        `settings titles, first ${came} after: ${firstFrame}`,
        `settings titles, ${stuck.ok ? "stuck" : "not stuck"} ` +
          `${stuck.ms} ms later: ${measure()}`,
      );
      if (!stuck.ok) {
        problems.push(
          "settings: the group titles do not stick (see the notes)",
        );
      }
    }
  } finally {
    useSettingsStore.getState().close();
  }
  return { problems, notes };
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
    await frameOrTick();
    await frameOrTick();
    await settles(
      () =>
        host.querySelectorAll("[data-rebase-row]").length === 3 &&
        !!host.querySelector("footer"),
    );
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
