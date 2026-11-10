import type { ReleaseSummary } from "../api/schemas.ts";

/** The choice of the release drawn, among those not pruned. */
export function ReleasePicker({
  releases,
  latest,
  chosen,
  onChoose,
}: {
  releases: ReleaseSummary[];
  latest: number;
  chosen: number;
  onChoose: (release: number) => void;
}) {
  return (
    <label className="flex items-center gap-1.5">
      <span className="text-dim max-lg:sr-only">Release</span>
      <select
        value={chosen}
        onChange={(event) => {
          onChoose(Number(event.target.value));
        }}
        className="rounded border border-line bg-space px-1.5 py-0.5"
      >
        {releases.map((release) => (
          <option key={release.number} value={release.number}>
            {release.number}
            {release.number === latest ? " (latest)" : ""}
          </option>
        ))}
      </select>
    </label>
  );
}
