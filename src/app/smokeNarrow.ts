import i18n from "../i18n";
import { commands } from "../ipc";
import {
  useGraphStore,
  WORKTREE,
  type OpenFile,
} from "../features/graph/store";
import { useRepoStore } from "../features/repo/store";
import { useSearchStore } from "../features/search/store";
import { frameOrTick, settles } from "./settle";
import { rebaseLayoutAt, type LayoutReport } from "./smokeLayout";

/* The interface in the narrowest window (smoke run): nothing outside it,
 * nothing cut, everything usable, in both languages. */

/** The minimum of tauri.conf.json (a test keeps both the same). */
export const NARROWEST = { width: 900, height: 560 };

const OVERFLOWS = new Set(["hidden", "auto", "scroll", "clip"]);

/** What can be seen of `element`, sideways: the window, cut by every ancestor
 * that clips. */
function visibleSpan(element: Element): { left: number; right: number } {
  let left = 0;
  let right = window.innerWidth;
  for (let up = element.parentElement; up; up = up.parentElement) {
    if (OVERFLOWS.has(getComputedStyle(up).overflowX)) {
      const box = up.getBoundingClientRect();
      left = Math.max(left, box.left);
      right = Math.min(right, box.right);
    }
  }
  return { left, right };
}

function name(element: Element): string {
  return (
    element.getAttribute("aria-label") ||
    element.textContent?.trim() ||
    element.tagName.toLowerCase()
  );
}

const shown = (element: Element) =>
  element.getBoundingClientRect().width > 0 &&
  getComputedStyle(element).visibility !== "hidden";

/** Controls that are out of sight sideways, wholly or in part. */
function cutControls(): string[] {
  return [
    ...document.querySelectorAll(
      "#root button, #root input, #root textarea, #root select",
    ),
  ]
    .filter(shown)
    .flatMap((control) => {
      const box = control.getBoundingClientRect();
      const span = visibleSpan(control);
      return box.left < span.left - 0.5 || box.right > span.right + 1
        ? [
            `"${name(control)}" (${Math.round(box.left)}–${Math.round(box.right)} of ${Math.round(span.left)}–${Math.round(span.right)})`,
          ]
        : [];
    });
}

/** Toolbar buttons in more than one line, or with their text cut. */
function badToolbarButtons(bar: Element): string[] {
  return [...bar.querySelectorAll("button")]
    .filter(shown)
    .filter(
      (b) =>
        b.scrollWidth > b.clientWidth + 1 ||
        b.scrollHeight > b.clientHeight + 1 ||
        b.getBoundingClientRect().height > 30,
    )
    .map(name);
}

/** What the toolbar offers: its buttons and what is in its "…" menu. */
async function toolbarOffers(bar: Element): Promise<string[]> {
  const buttons = [...bar.querySelectorAll("button")].filter(shown);
  const offers = buttons.map(name);
  const more = buttons.find((b) => name(b) === i18n.t("toolbar.more"));
  if (more) {
    more.click();
    const menu = () => document.querySelector('[role="menu"]');
    await settles(() => menu() !== null);
    offers.push(
      ...[...(menu()?.querySelectorAll("button") ?? [])].map((b) => name(b)),
    );
    window.dispatchEvent(new Event("blur"));
    await settles(() => menu() === null);
  }
  return offers;
}

async function problemsNow(where: string): Promise<LayoutReport> {
  const problems: string[] = [];
  const bar = document.querySelector("header");
  const detail = [...document.querySelectorAll("aside")].at(-1);
  if (!bar || !detail) {
    return { problems: [`${where}: the interface is missing`], notes: [] };
  }
  const cut = cutControls();
  if (cut.length > 0) problems.push(`${where}: cut: ${cut.join(", ")}`);
  const bad = badToolbarButtons(bar);
  if (bad.length > 0) {
    problems.push(
      `${where}: toolbar text that does not fit: ${bad.join(", ")}`,
    );
  }
  if (detail.getBoundingClientRect().right > window.innerWidth + 1) {
    problems.push(`${where}: the right panel goes out of the window`);
  }
  const offers = await toolbarOffers(bar);
  const missing = [
    "toolbar.openRepo",
    "toolbar.fetch",
    "toolbar.pull",
    "toolbar.push",
    "publish.button",
    "toolbar.branch",
    "toolbar.stash",
    "undo.toolbar.undo",
    "undo.toolbar.redo",
    "settings.title",
    "operations.button",
  ]
    .map((key) => i18n.t(key as never) as string)
    .filter((text) => !offers.some((offer) => offer.startsWith(text)));
  if (missing.length > 0) {
    problems.push(`${where}: nowhere in the toolbar: ${missing.join(", ")}`);
  }
  const settings = [...bar.querySelectorAll("button")].find(
    (b) => name(b) === i18n.t("settings.title"),
  );
  if (!settings || !shown(settings)) {
    problems.push(`${where}: Settings is not in the toolbar itself`);
  }
  // The summary and the labels of the history.
  const rows = [...document.querySelectorAll('[role="option"]')];
  const descriptions = rows.map(
    (row) => row.firstElementChild?.getBoundingClientRect().width ?? 0,
  );
  const narrowest = Math.round(Math.min(...descriptions));
  if (rows.length === 0 || narrowest < 200) {
    problems.push(`${where}: the descriptions are ${narrowest}px wide`);
  }
  const labels = rows.flatMap((row) => [
    ...row.querySelectorAll<HTMLElement>("[data-drop-branch]"),
  ]);
  const squeezed = labels.filter(
    (l) => l.clientWidth < Math.min(l.scrollWidth, 50),
  );
  if (labels.length === 0 || squeezed.length > 0) {
    problems.push(
      `${where}: labels squeezed: ${squeezed.map((l) => `${name(l)} ${l.clientWidth}px`).join(", ") || "there are none"}`,
    );
  }
  // The commit button, also from top to bottom.
  const commit = document.querySelector('form button[type="submit"]');
  if (commit) {
    const box = commit.getBoundingClientRect();
    if (box.bottom > window.innerHeight + 0.5 || box.height < 20) {
      problems.push(`${where}: the commit button is out of sight`);
    }
  }
  return {
    problems,
    notes: [
      `${where}: ${window.innerWidth}×${window.innerHeight}, ` +
        `dropped from the toolbar: ${bar.dataset.dropped || "nothing"}; ` +
        `history ${Math.round(document.querySelector("main")!.getBoundingClientRect().width)}px, ` +
        `descriptions from ${narrowest}px, right panel ${Math.round(detail.getBoundingClientRect().width)}px`,
    ],
  };
}

/** Opens a repository in a tab; its id and newest commit once its history is
 * there. */
async function open(path: string) {
  await useRepoStore.getState().openRepository(path);
  const id = useRepoStore.getState().activeId;
  if (id === null) return null;
  const graph = () => useGraphStore.getState().graphs[id];
  await settles(() => graph()?.status === "done" && !!graph()?.refs, 15000);
  const newest = graph()?.lines[0]?.oid;
  return newest ? { id, newest } : null;
}

/** Waits for `selector` and looks for anything cut. */
async function cutIn(where: string, selector: string): Promise<string[]> {
  const there = await settles(
    () => document.querySelector(selector) !== null,
    10000,
  );
  if (!there.ok) return [`${where}: it did not open`];
  await frameOrTick();
  await frameOrTick();
  const cut = cutControls();
  return cut.length > 0 ? [`${where}: cut: ${cut.join(", ")}`] : [];
}

/** The views that take the history's place, and the search. */
async function viewProblems(
  where: string,
  id: number,
  newest: string,
): Promise<string[]> {
  const graph = useGraphStore.getState();
  const path = "changed-file-with-a-long-name.txt";
  const problems: string[] = [];
  const files: [string, OpenFile][] = [
    ["diff", { kind: "worktree", area: "unstaged", path, oldPath: null }],
    ["history of a file", { kind: "history", path, rev: "HEAD", chosen: null }],
    ["blame", { kind: "blame", path, rev: newest, chosen: null, back: [] }],
  ];
  graph.select(id, WORKTREE);
  for (const [what, file] of files) {
    graph.openFileDiff(id, file);
    // Its content, not only its frame.
    const content =
      file.kind === "worktree"
        ? "main table"
        : "main section li, main section [role]";
    problems.push(...(await cutIn(`${where}, ${what}`, content)));
    graph.closeFile(id);
  }
  useSearchStore.getState().openBar(id);
  problems.push(...(await cutIn(`${where}, search`, "main input")));
  useSearchStore.getState().close(id);
  return problems;
}

/** The file in conflict of the second sample. */
async function conflictProblems(where: string, id: number): Promise<string[]> {
  const graph = useGraphStore.getState();
  graph.select(id, WORKTREE);
  const path = "changed-file-with-a-long-name.txt";
  const problems = await cutIn(
    `${where}, conflict panel`,
    "main [role], main button",
  );
  graph.openFileDiff(id, { kind: "conflict", path });
  problems.push(...(await cutIn(`${where}, conflict`, "main section header")));
  graph.closeFile(id);
  return problems;
}

export async function narrowLayout(): Promise<LayoutReport> {
  const report: LayoutReport = { problems: [], notes: [] };
  const sample = await commands.smokeSampleRepository();
  const conflict = await commands.smokeConflict();
  if (
    sample.status === "error" ||
    conflict.status === "error" ||
    !sample.data ||
    !conflict.data
  ) {
    const why = [sample, conflict].map((r) =>
      r.status === "error" ? r.error.message : "made",
    );
    return {
      problems: [`narrow: sample repositories: ${why.join("; ")}`],
      notes: [],
    };
  }
  await commands.smokeResize(NARROWEST.width, NARROWEST.height);
  const resized = await settles(
    () => Math.abs(window.innerWidth - NARROWEST.width) <= 2,
    5000,
  );
  if (!resized.ok) {
    report.problems.push(
      `narrow: the window is ${window.innerWidth}×${window.innerHeight}`,
    );
  }
  const language = i18n.language;
  const languages = ["es", "en"] as const;
  try {
    const main = await open(sample.data);
    if (!main) {
      report.problems.push("narrow: the sample repository did not open");
      return report;
    }
    for (const lang of languages) {
      await i18n.changeLanguage(lang);
      for (const [what, selection, there] of [
        ["commit", main.newest, "aside dl"],
        ["changes", WORKTREE, 'aside form button[type="submit"]'],
      ] as const) {
        useGraphStore.getState().select(main.id, selection);
        await settles(() => document.querySelector(there) !== null, 10000);
        await frameOrTick();
        await frameOrTick();
        const now = await problemsNow(`narrow, ${lang}, ${what}`);
        report.problems.push(...now.problems);
        report.notes.push(...now.notes);
      }
      const width = document.querySelector("main")?.clientWidth ?? 0;
      report.problems.push(...(await rebaseLayoutAt(width)));
      report.problems.push(
        ...(await viewProblems(`narrow, ${lang}`, main.id, main.newest)),
      );
    }
    const stopped = await open(conflict.data);
    if (!stopped) {
      report.problems.push("narrow: the conflict sample did not open");
      return report;
    }
    for (const lang of languages) {
      await i18n.changeLanguage(lang);
      report.problems.push(
        ...(await conflictProblems(`narrow, ${lang}`, stopped.id)),
      );
    }
  } finally {
    await i18n.changeLanguage(language);
  }
  return report;
}
