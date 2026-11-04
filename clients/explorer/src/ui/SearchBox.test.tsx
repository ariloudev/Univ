import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";

import { ANDROMEDA } from "../test/objects.ts";
import { SearchBox } from "./SearchBox.tsx";

function answer(status: number, body: unknown) {
  const fetch = vi.fn((..._args: unknown[]) => Promise.resolve(Response.json(body, { status })));
  vi.stubGlobal("fetch", fetch);
  return fetch;
}

async function find(query: string) {
  const found = vi.fn();
  render(<SearchBox release={7} onFound={found} />);
  await userEvent.type(screen.getByRole("textbox"), query);
  await userEvent.click(screen.getByRole("button", { name: "Find" }));
  return found;
}

afterEach(() => {
  vi.unstubAllGlobals();
});

describe("searching", () => {
  it("makes the object found the chosen one", async () => {
    const fetch = answer(200, ANDROMEDA);
    const found = await find("Andromeda");
    expect(found).toHaveBeenCalledWith(856254, ANDROMEDA);
    expect(fetch.mock.calls[0]?.[0]).toBe("/v1/objects?name=Andromeda&release=7");
  });

  it("lists the objects a name belongs to, to choose from", async () => {
    answer(300, { objects: [{ id: 1, href: "/v1/objects/1", names: ["Twin"] }, { id: 2, href: "/v1/objects/2", names: ["Twin"] }] });
    const found = await find("Twin");
    expect(await screen.findByText("“Twin” names 2 objects:")).toBeTruthy();
    await userEvent.click(screen.getByRole("button", { name: "2 (Twin)" }));
    expect(found).toHaveBeenCalledWith(2);
  });

  it("repeats what was searched for when nothing is found", async () => {
    answer(404, { status: 404, title: "No such object", type: "urn:univ:problem:object-not-found" });
    await find("Nowhere");
    expect((await screen.findByRole("status")).textContent).toBe("Nothing found for “Nowhere”.");
  });
});
