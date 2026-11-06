import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";

import { UNREACHABLE } from "../api/client.ts";
import { App } from "./App.tsx";
import { START_COMMAND } from "./ServerUnreachable.tsx";

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
const refused = () =>
  Promise.resolve(
    new Response(
      JSON.stringify({ type: UNREACHABLE, title: "Univ Server is not answering", address: "http://127.0.0.1:8790" }),
      { status: 502, headers: { "content-type": "application/problem+json" } },
    ),
  );

afterEach(() => {
  vi.unstubAllGlobals();
  history.replaceState(null, "", "/");
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

  it("holds a level-of-detail switch in its top bar, on by default, that turns it off and on", async () => {
    serve(ok);
    render(<App />);
    const toggle = await screen.findByRole("switch", { name: "Level of detail" });
    expect(toggle.closest("header")).not.toBeNull();
    expect((toggle as HTMLInputElement).checked).toBe(true);
    await userEvent.click(toggle);
    expect((toggle as HTMLInputElement).checked).toBe(false);
    expect(location.search).toContain("lod=off");
    await userEvent.click(toggle);
    expect((toggle as HTMLInputElement).checked).toBe(true);
    expect(location.search).not.toContain("lod");
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

  it("names the problem when Univ Server refuses to list its releases, and tries again", async () => {
    const problem = new Response(JSON.stringify({ status: 404, title: "No such release" }), {
      status: 404,
      headers: { "content-type": "application/problem+json" },
    });
    serve(() => Promise.resolve(problem), ok);
    render(<App />);
    expect((await screen.findByRole("alert")).textContent).toContain("404 No such release");
    await userEvent.click(screen.getByRole("button", { name: "Try again" }));
    expect(await screen.findByText(/Drawing release 7/)).toBeTruthy();
  });
});
