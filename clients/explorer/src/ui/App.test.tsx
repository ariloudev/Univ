import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";

import { App } from "./App.tsx";
import { START_COMMAND } from "./Unreachable.tsx";

const cosmology = { name: "Planck 2018", hubble_constant: 67.66, omega_matter: 0.3111 };
const RELEASES = {
  latest: 7,
  releases: [
    { number: 7, published_at: "2026-11-04T02:15:00Z", pruned_at: null, cosmology },
    { number: 6, published_at: "2026-11-03T18:00:00Z", pruned_at: null, cosmology },
    { number: 4, published_at: "2026-11-03T12:00:00Z", pruned_at: "2026-11-03T17:30:08Z", cosmology },
  ],
};

function serve(...answers: (() => Promise<Response>)[]) {
  const fetch = vi.fn((..._args: unknown[]) => {
    const next = answers.shift();
    if (next === undefined) throw new Error("no more answers");
    return next();
  });
  vi.stubGlobal("fetch", fetch);
  return fetch;
}

const ok = () => Promise.resolve(new Response(JSON.stringify(RELEASES), { status: 200 }));
const refused = () => Promise.reject(new TypeError("Failed to fetch"));

afterEach(() => {
  vi.unstubAllGlobals();
});

describe("opening the explorer", () => {
  it("draws the release latest points to, and names it", async () => {
    serve(ok);
    render(<App />);
    expect(await screen.findByText(/Drawing release 7, Planck 2018/)).toBeTruthy();
    expect(screen.getByRole("region", { name: "The view" }).dataset["release"]).toBe("7");
  });

  it("offers every release not pruned, and draws the one chosen", async () => {
    serve(ok);
    render(<App />);
    const picker = await screen.findByRole("combobox");
    const offered = [...picker.querySelectorAll("option")].map((o) => o.textContent);
    expect(offered).toEqual(["7 (latest)", "6"]);
    await userEvent.selectOptions(picker, "6");
    expect(screen.getByText(/Drawing release 6/)).toBeTruthy();
    expect(screen.getByRole("region", { name: "The view" }).dataset["release"]).toBe("6");
  });

  it("says where it looked and how to start Univ Server, and tries again when asked", async () => {
    const fetch = serve(refused, ok);
    render(<App />);
    expect(await screen.findByText("Univ Server is not answering")).toBeTruthy();
    expect(screen.getByText("http://127.0.0.1:8790")).toBeTruthy();
    expect(screen.getByText(START_COMMAND)).toBeTruthy();
    await userEvent.click(screen.getByRole("button", { name: "Try again" }));
    expect(await screen.findByText(/Drawing release 7/)).toBeTruthy();
    expect(fetch).toHaveBeenCalledTimes(2);
  });

  it("names the problem when Univ Server refuses to list its releases", async () => {
    const problem = new Response(JSON.stringify({ status: 404, title: "No such release" }), {
      status: 404,
      headers: { "content-type": "application/problem+json" },
    });
    serve(() => Promise.resolve(problem));
    render(<App />);
    expect((await screen.findByRole("alert")).textContent).toContain("404 No such release");
  });
});
