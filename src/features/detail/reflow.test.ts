import { reflowMessage } from "./reflow";

/*
 * Bodies below follow the shapes of real commit messages (Git, Linux and
 * Rust projects): 72-column wrapping, lists, indented logs and trailers.
 */

describe("reflowMessage", () => {
  it("joins the wrapped lines of each paragraph", () => {
    const body = [
      "The previous implementation walked the whole history before showing",
      "anything, which made large repositories feel frozen for seconds.",
      "",
      "Stream the commits instead and lay them out as they arrive, so the",
      "first screen is ready almost immediately.",
    ].join("\n");
    expect(reflowMessage(body)).toBe(
      [
        "The previous implementation walked the whole history before showing anything, which made large repositories feel frozen for seconds.",
        "",
        "Stream the commits instead and lay them out as they arrive, so the first screen is ready almost immediately.",
      ].join("\n"),
    );
  });

  it("keeps list items on their own lines and joins their continuations", () => {
    const body = [
      "This series does three things:",
      "",
      "- teaches the parser about renames, so that moved files are no",
      "  longer shown as a deletion plus an addition;",
      "- adds tests;",
      "* fixes a typo in the",
      "documentation.",
      "",
      "1. first step",
      "2) second step that is long enough to",
      "   wrap onto another line",
    ].join("\n");
    expect(reflowMessage(body)).toBe(
      [
        "This series does three things:",
        "",
        "- teaches the parser about renames, so that moved files are no longer shown as a deletion plus an addition;",
        "- adds tests;",
        "* fixes a typo in the documentation.",
        "",
        "1. first step",
        "2) second step that is long enough to wrap onto another line",
      ].join("\n"),
    );
  });

  it("keeps a list that follows prose without a blank line", () => {
    expect(reflowMessage("Changes:\n- one\n- two")).toBe(
      "Changes:\n- one\n- two",
    );
  });

  it("leaves indented blocks such as logs and code untouched", () => {
    const body = [
      "The crash looked like this:",
      "",
      "    thread 'main' panicked at src/main.rs:10:5:",
      "    index out of bounds: the len is 0",
      "\tat foo (bar.c:12)",
      "",
      "Check the length first.",
    ].join("\n");
    expect(reflowMessage(body)).toBe(body);
  });

  it("keeps trailers one per line", () => {
    const body = [
      "Fix the off-by-one in the hunk header parser when the old range",
      "is empty.",
      "",
      "Reported-by: Ana García <ana@example.com>",
      "Reviewed-by: Bea <bea@example.com>",
      "Signed-off-by: Carla <carla@example.com>",
    ].join("\n");
    expect(reflowMessage(body)).toBe(
      [
        "Fix the off-by-one in the hunk header parser when the old range is empty.",
        "",
        "Reported-by: Ana García <ana@example.com>",
        "Reviewed-by: Bea <bea@example.com>",
        "Signed-off-by: Carla <carla@example.com>",
      ].join("\n"),
    );
  });

  it("normalises line endings, extra blank lines and surrounding space", () => {
    expect(reflowMessage("\r\nfirst\r\nline\r\n\r\n\r\n\r\nsecond  \n")).toBe(
      "first line\n\nsecond",
    );
    expect(reflowMessage("")).toBe("");
  });

  it("does not treat a hyphenated word or a version as a list", () => {
    expect(reflowMessage("Bump to\n-rc2 of the\n2.5 series")).toBe(
      "Bump to -rc2 of the 2.5 series",
    );
  });
});
