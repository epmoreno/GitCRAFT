import { fireEvent, render, screen } from "@testing-library/react";
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
    expect(placeTooltip(box(400, 570), tip, viewport).side).toBe("above");
  });

  it("stays below when neither side has room but below has more", () => {
    const tall = { width: 200, height: 500 };
    expect(placeTooltip(box(400, 100), tall, viewport).side).toBe("below");
  });

  it("is placed when it is about to show", () => {
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

    fireEvent.pointerEnter(host);
    expect(tooltip.style.getPropertyValue("--tip-shift")).toBe("-68px");
    expect(button).toHaveAccessibleDescription("Ver las operaciones");
  });
});
