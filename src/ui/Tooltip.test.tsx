import { act, fireEvent, render, screen } from "@testing-library/react";
import { placeTooltip, Tooltip, TOOLTIP_MARGIN } from "./Tooltip";

const viewport = { width: 1000, height: 600 };
const tip = { width: 200, height: 24 };
const box = (left: number, top: number, width = 80, height = 28) => ({
  left,
  top,
  right: left + width,
  bottom: top + height,
});

describe("tooltip placement", () => {
  it("stays centred below the host when it fits", () => {
    expect(placeTooltip(box(400, 10), tip, viewport)).toEqual({
      side: "below",
      shift: 0,
      left: 340,
      top: 44,
    });
  });

  it("moves left to stay inside the right edge", () => {
    // A host at the right edge (like "Operations"): centred, the tooltip
    // would end at 1000 - 40 + 100 = 1060.
    const host = box(920, 10);
    const { shift } = placeTooltip(host, tip, viewport);
    const right = (host.left + host.right) / 2 + tip.width / 2 + shift;
    expect(right).toBe(viewport.width - TOOLTIP_MARGIN);
  });

  it("moves right to stay inside the left edge", () => {
    const host = box(0, 10, 40);
    const { shift } = placeTooltip(host, tip, viewport);
    expect(20 - tip.width / 2 + shift).toBe(TOOLTIP_MARGIN);
  });

  it("goes above when there is no room below", () => {
    expect(placeTooltip(box(400, 570), tip, viewport)).toMatchObject({
      side: "above",
      top: 570 - 6 - 24,
    });
  });

  it("stays below when neither side has room but below has more", () => {
    const tall = { width: 200, height: 500 };
    expect(placeTooltip(box(400, 100), tall, viewport).side).toBe("below");
  });

  it("is placed in the window when it is about to show", () => {
    Object.defineProperty(window, "innerWidth", {
      configurable: true,
      value: 1000,
    });
    render(
      <Tooltip label="Ver las operaciones">
        <button type="button">Operaciones</button>
      </Tooltip>,
    );
    const button = screen.getByRole("button", { name: "Operaciones" });
    const host = button.parentElement!;
    const tooltip = screen.getByRole("tooltip", { hidden: true });
    vi.spyOn(host, "getBoundingClientRect").mockReturnValue(
      box(920, 10) as DOMRect,
    );
    Object.defineProperty(tooltip, "offsetWidth", { value: 200 });
    Object.defineProperty(tooltip, "offsetHeight", { value: 24 });

    expect(tooltip).not.toHaveClass("shown");
    fireEvent.pointerEnter(host);
    expect(tooltip).toHaveClass("shown");
    expect(tooltip.style.left).toBe(`${1000 - TOOLTIP_MARGIN - 200}px`);
    expect(tooltip.style.top).toBe("44px");
    expect(button).toHaveAccessibleDescription("Ver las operaciones");
    fireEvent.pointerLeave(host);
    expect(tooltip).not.toHaveClass("shown");
  });

  it("is not cut by a panel that clips its content", () => {
    render(
      <div data-testid="panel" style={{ overflow: "hidden", width: 120 }}>
        <Tooltip label="«main» ya está en este commit">
          <button type="button" disabled>
            Restablecer…
          </button>
        </Tooltip>
      </div>,
    );
    const tooltip = screen.getByRole("tooltip", { hidden: true });
    // Its own layer, outside the panel.
    expect(screen.getByTestId("panel")).not.toContainElement(tooltip);
    expect(tooltip.parentElement).toBe(document.body);
    expect(
      screen.getByRole("button", { name: "Restablecer…" }),
    ).toHaveAccessibleDescription("«main» ya está en este commit");
  });

  it("shows on focus and hides when anything scrolls", () => {
    render(
      <Tooltip label="Deshacer">
        <button type="button">↶</button>
      </Tooltip>,
    );
    const tooltip = screen.getByRole("tooltip", { hidden: true });
    act(() => screen.getByRole("button").focus());
    expect(tooltip).toHaveClass("shown");
    fireEvent.scroll(window);
    expect(tooltip).not.toHaveClass("shown");
  });
});
