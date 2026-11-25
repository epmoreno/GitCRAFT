import { act, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { version } from "../../../package.json";
import i18n from "../../i18n";
import { appVersion } from "../../ipc";
import { ipc } from "../../test/ipc";
import { AboutDialog } from "./AboutDialog";
import { useAboutStore } from "./store";

describe("About GitCRAFT", () => {
  beforeEach(() => {
    vi.mocked(appVersion).mockResolvedValue(version);
  });

  it.each([
    ["es", "Acerca de GitCRAFT", "Licencia MIT"],
    ["en", "About GitCRAFT", "MIT License"],
  ])(
    "in %s, shows the name, version, licence and repository",
    async (language, title, license) => {
      await i18n.changeLanguage(language);
      const user = userEvent.setup();
      render(<AboutDialog />);
      expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
      act(() => useAboutStore.getState().open());
      const dialog = screen.getByRole("dialog", { name: title });
      expect(await within(dialog).findByText(version)).toHaveClass(
        "selectable",
      );
      expect(within(dialog).getByText("GitCRAFT")).toBeVisible();
      expect(dialog).toHaveTextContent(license);
      await user.click(
        within(dialog).getByRole("button", {
          name: "github.com/epmoreno/GitCRAFT",
        }),
      );
      expect(ipc.openWebPage).toHaveBeenCalledWith("project");
      await user.keyboard("{Escape}");
      expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    },
  );
});
