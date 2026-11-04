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
    <label className="flex items-center gap-2 text-sm">
      <span className="text-dim">Release</span>
      <select
        value={chosen}
        onChange={(event) => {
          onChoose(Number(event.target.value));
        }}
        className="rounded border border-line bg-space px-2 py-1"
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
