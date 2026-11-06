import { useState } from "react";

import type { ReleaseSummary } from "../api/schemas.ts";
import { ReleasePicker } from "./ReleasePicker.tsx";
import { Scene } from "./Scene.tsx";
import { ServerUnreachable, TryAgain } from "./ServerUnreachable.tsx";
import { useLevelOfDetail } from "./levelOfDetail.ts";
import { useReleases } from "./useReleases.ts";

/** Univ Explorer: the releases of Univ Server, and the view of the one chosen. */
export function App() {
  const { state, retry } = useReleases();
  switch (state.kind) {
    case "loading":
      return (
        <p role="status" className="p-6 text-dim">
          {state.waiting === undefined
            ? "Reading Univ Server's releases…"
            : `Univ Server's rate limit was reached: reading again in ${state.waiting} s…`}
        </p>
      );
    case "unreachable":
      return <ServerUnreachable address={state.address} onRetry={retry} />;
    case "failed":
      return (
        <div role="alert" className="space-y-3 p-6 text-warn">
          <p>Univ Server could not list its releases: {state.message}</p>
          <TryAgain onRetry={retry} />
        </div>
      );
    case "ready":
      return <Explorer releases={state.releases} latest={state.latest} />;
  }
}

function Explorer({ releases, latest }: { releases: ReleaseSummary[]; latest: number }) {
  const [release, setRelease] = useState(latest);
  const [levelOfDetail, setLevelOfDetail] = useLevelOfDetail();
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
        <label className="ml-auto flex items-center gap-2 text-sm">
          <input
            type="checkbox"
            role="switch"
            checked={levelOfDetail}
            onChange={(event) => {
              setLevelOfDetail(event.currentTarget.checked);
            }}
          />
          Level of detail
        </label>
      </header>
      <section aria-label="The view" className="relative flex-1" data-release={release}>
        <Scene key={release} release={release} cosmology={shown?.cosmology.name ?? ""} levelOfDetail={levelOfDetail} />
      </section>
    </div>
  );
}
