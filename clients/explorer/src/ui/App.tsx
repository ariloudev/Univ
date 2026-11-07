import { useState } from "react";

import type { ReleaseSummary } from "../api/schemas.ts";
import { AtlasPage } from "./Atlas.tsx";
import type { DrawerName } from "./Rail.tsx";
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

/** A cosmology's name without the citation in brackets after it, which its title gives. */
const shortName = (name: string): string => name.replace(/\s*\(.*$/, "");

function Explorer({ releases, latest }: { releases: ReleaseSummary[]; latest: number }) {
  const [release, setRelease] = useState(latest);
  const [levelOfDetail, setLevelOfDetail] = useLevelOfDetail();
  // The top bar's place for the view's parts, and the drawer open, outlive each release's view.
  const [topBar, setTopBar] = useState<HTMLElement | null>(null);
  const [drawer, setDrawer] = useState<DrawerName>();
  const shown = releases.find((r) => r.number === release);
  const start = (
    <>
      <h1 className="font-serif text-base max-xl:sr-only">Univ Explorer</h1>
      <ReleasePicker releases={releases} latest={latest} chosen={release} onChoose={setRelease} />
      <p role="status" className="text-dim max-lg:sr-only" title={shown?.cosmology.name}>
        Drawing release {release}
        {shown === undefined ? "" : <span className="max-xl:sr-only">, {shortName(shown.cosmology.name)}</span>}
      </p>
    </>
  );
  const end = (
    <label className="flex items-center gap-2">
      <input
        type="checkbox"
        role="switch"
        checked={levelOfDetail}
        onChange={(event) => {
          setLevelOfDetail(event.currentTarget.checked);
        }}
        className="accent-accent"
      />
      <span className="max-lg:sr-only">Level of detail</span>
    </label>
  );
  return (
    <AtlasPage start={start} end={end} slot={setTopBar}>
      <Scene
        key={release}
        release={release}
        cosmology={shown?.cosmology.name ?? ""}
        levelOfDetail={levelOfDetail}
        place={{ topBar, drawer, onDrawer: setDrawer }}
      />
    </AtlasPage>
  );
}
