import { render, screen } from "@testing-library/react";
import i18n from "../i18n";
import { App } from "./App";

describe("App shell", () => {
  beforeEach(async () => {
    await i18n.changeLanguage("es");
  });

  it("renders the three zones", () => {
    render(<App />);
    expect(
      screen.getByRole("complementary", { name: "sidebar" }),
    ).toBeVisible();
    expect(screen.getByRole("main", { name: "graph" })).toBeVisible();
    expect(screen.getByRole("complementary", { name: "detail" })).toBeVisible();
  });

  it("shows the empty state and disabled repo actions without a repo", () => {
    render(<App />);
    expect(screen.getByText("Ningún repositorio abierto")).toBeVisible();
    for (const label of ["Pull", "Push", "Rama", "Stash", "Deshacer"]) {
      expect(screen.getByRole("button", { name: label })).toBeDisabled();
    }
  });

  it("switches language", async () => {
    await i18n.changeLanguage("en");
    render(<App />);
    expect(screen.getByText("No repository open")).toBeVisible();
  });
});
