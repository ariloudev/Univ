import { useState } from "react";

import type { ReleaseSummary } from "../api/schemas.ts";
import { ReleasePicker } from "./ReleasePicker.tsx";
import { Unreachable } from "./Unreachable.tsx";
import { useReleases } from "./useReleases.ts";

/** Univ Explorer: the releases of Univ Server, and the view of the one chosen. */
export function App() {
  const { state, retry } = useReleases();
  switch (state.kind) {
    case "loading":
      return <p role="status" className="p-6 text-dim">Reading Univ Server's releases…</p>;
    case "unreachable":
      return <Unreachable address={state.address} onRetry={retry} />;
    case "failed":
      return (
        <p role="alert" className="p-6 text-warn">
          Univ Server could not list its releases: {state.message}
        </p>
      );
    case "ready":
      return <Explorer releases={state.releases} latest={state.latest} />;
  }
}

function Explorer({ releases, latest }: { releases: ReleaseSummary[]; latest: number }) {
  const [release, setRelease] = useState(latest);
  const shown = releases.find((r) => r.number === release);
  return (
    <div className="flex h-full flex-col">
      <header className="flex items-center gap-4 border-b border-line bg-panel px-4 py-2">
        <h1 className="font-semibold">Univ Explorer</h1>
        <ReleasePicker releases={releases} latest={latest} chosen={release} onChoose={setRelease} />
        <p role="status" className="text-sm text-dim">
          Drawing release {release}
          {shown === undefined ? "" : `, ${shown.cosmology.name}`}
        </p>
      </header>
      <section aria-label="The view" className="relative flex-1" data-release={release} />
    </div>
  );
}
