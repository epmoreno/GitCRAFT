import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import i18n, { resources } from "../i18n";
import { commandError } from "../test/ipc";
import { ERROR_KINDS, ErrorReport } from "./ErrorReport";

describe("ErrorReport", () => {
  beforeEach(async () => {
    await i18n.changeLanguage("es");
  });

  it("has an explanation for every error kind in every language", () => {
    for (const [language, { translation }] of Object.entries(resources)) {
      const errors = translation.errors as Record<string, string>;
      for (const kind of ERROR_KINDS) {
        expect(errors[kind], `${language}: errors.${kind}`).toBeTruthy();
        // A message that names a folder also reads well without one.
        if (errors[kind]?.includes("{{path}}")) {
          expect(
            errors[`${kind}_noPath`],
            `${language}: ${kind}_noPath`,
          ).toBeTruthy();
          expect(errors[`${kind}_noPath`]).not.toContain("{{path}}");
        }
      }
    }
  });

  it("explains, then shows and copies git's original output", async () => {
    const user = userEvent.setup();
    const writeText = vi.fn().mockResolvedValue(undefined);
    Object.defineProperty(navigator, "clipboard", {
      value: { writeText },
      configurable: true,
    });
    const output = "$ git push\nfatal: Authentication failed\n(exit code 128)";
    render(
      <ErrorReport
        error={commandError("authFailed", output)}
        lead="No se pudo hacer push."
      />,
    );
    const alert = screen.getByRole("alert");
    expect(alert).toHaveTextContent(
      /^No se pudo hacer push\. Git no pudo autenticarse con el remoto/,
    );
    const raw = within(alert).getByText(/Authentication failed/);
    expect(raw).not.toBeVisible();

    await user.click(within(alert).getByText("Ver la salida original de Git"));
    expect(raw).toBeVisible();
    expect(raw).toHaveClass("selectable");
    await user.click(
      within(alert).getByRole("button", { name: "Copiar la salida" }),
    );
    expect(writeText).toHaveBeenCalledWith(output);
    expect(
      await within(alert).findByRole("button", { name: "Copiada" }),
    ).toBeVisible();
  });

  it("does not show an empty folder name", () => {
    render(<ErrorReport error={commandError("notARepository")} />);
    expect(screen.getByRole("alert")).toHaveTextContent(
      "Esa carpeta no es un repositorio Git.",
    );
    render(
      <ErrorReport error={commandError("notARepository")} path="/x/fotos" />,
    );
    expect(screen.getAllByRole("alert")[1]).toHaveTextContent(
      "La carpeta «/x/fotos» no es un repositorio Git.",
    );
  });

  it("shows the technical message of an unknown error without output", () => {
    render(
      <ErrorReport
        error={{
          kind: "unknown",
          message: "unexpected git output: bad line",
          details: null,
        }}
      />,
    );
    const alert = screen.getByRole("alert");
    expect(alert).toHaveTextContent(/no reconoce/);
    expect(within(alert).getByText(/bad line/)).toHaveClass("selectable");
  });

  it("offers nothing to expand when there is no output", () => {
    render(<ErrorReport error={commandError("nothingToStash")} />);
    expect(screen.getByRole("alert")).toHaveTextContent(
      "No hay cambios sin confirmar que guardar.",
    );
    expect(
      screen.queryByText("Ver la salida original de Git"),
    ).not.toBeInTheDocument();
  });
});
