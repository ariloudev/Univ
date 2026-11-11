import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";

import { Rail } from "./Rail.tsx";

const none = () => undefined;

describe("the rail (see-the-galaxys-arms-face-on)", () => {
  it("offers the arms view beside the control that returns to Earth", async () => {
    const onArms = vi.fn();
    render(<Rail open={undefined} onToggle={none} onEarth={none} onArms={onArms} />);
    const arms = screen.getByRole("button", { name: "See the Galaxy's arms face-on" });
    const earth = screen.getByRole("button", { name: "Return to Earth" });
    expect(arms.nextElementSibling).toBe(earth);
    await userEvent.click(arms);
    expect(onArms).toHaveBeenCalledOnce();
  });

  it("offers no arms view where the view has none to open", () => {
    render(<Rail open={undefined} onToggle={none} onEarth={none} />);
    expect(screen.queryByRole("button", { name: "See the Galaxy's arms face-on" })).toBeNull();
    expect(screen.getByRole("button", { name: "Return to Earth" }).className).toContain("mt-auto");
  });
});
