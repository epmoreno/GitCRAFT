import i18n from "../../i18n";
import { describeOp } from "./describe";

describe("describeOp", () => {
  afterEach(async () => {
    await i18n.changeLanguage("es");
  });

  it("names the interactive rebase by its branch, in both languages", async () => {
    const kind = { type: "interactiveRebase", branch: "main" } as const;
    await i18n.changeLanguage("es");
    expect(describeOp(i18n.t, kind)).toBe("el rebase interactivo de «main»");
    await i18n.changeLanguage("en");
    expect(describeOp(i18n.t, kind)).toBe("the interactive rebase of “main”");
  });
});
