import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";

import { RecordPanel } from "./RecordPanel.tsx";

describe("the base record panel", () => {
  it("is headed by its name and other names, says what it is, and closes", async () => {
    const onClose = vi.fn();
    render(
      <RecordPanel label="The chosen object" title="HIP 74676" subtitle="Gaia DR3 5886734991133223296 · object 377810" lede="An A-type star." closeLabel="Close the object's panel" onClose={onClose} fullLabel="Show every value" full={() => "all"}>
        <p>sections</p>
      </RecordPanel>,
    );
    const panel = screen.getByRole("region", { name: "The chosen object" });
    expect(screen.getByRole("heading", { name: "HIP 74676" }).textContent).toBe("HIP 74676");
    expect(panel.textContent).toContain("Gaia DR3 5886734991133223296 · object 377810");
    expect(screen.getByTestId("lede").textContent).toBe("An A-type star.");
    await userEvent.click(screen.getByRole("button", { name: "Close the object's panel" }));
    expect(onClose).toHaveBeenCalledOnce();
  });

  it("makes the whole record only once its link opens it", async () => {
    const full = vi.fn(() => <p>every value</p>);
    render(
      <RecordPanel label="The chosen structure" title="Vela Supercluster" italic closeLabel="Close" onClose={() => undefined} fullLabel="Show the record's notes and quotes" full={full}>
        {null}
      </RecordPanel>,
    );
    expect(screen.getByRole("heading", { name: "Vela Supercluster" }).className).toContain("italic");
    expect(full).not.toHaveBeenCalled();
    await userEvent.click(screen.getByText("Show the record's notes and quotes"));
    expect((await screen.findByText("every value")).tagName).toBe("P");
  });
});
