import { type FormEvent, useState } from "react";

import { type Found, type ObjectRecord, search } from "../api/objects.ts";

type State = { kind: "idle" } | { kind: "searching" } | { kind: "failed"; message: string } | Exclude<Found, { kind: "object" }>;

/** Finds an object by id, designation (`source:identifier`) or name (find-an-object). */
export function SearchBox({ release, onFound }: { release: number; onFound: (id: number, record?: ObjectRecord) => void }) {
  const [query, setQuery] = useState("");
  const [state, setState] = useState<State>({ kind: "idle" });
  const submit = (event: FormEvent) => {
    event.preventDefault();
    if (query.trim() === "") return;
    setState({ kind: "searching" });
    search(query, { release })
      .then((found) => {
        if (found.kind === "object") {
          setState({ kind: "idle" });
          onFound(found.record.id, found.record);
        } else setState(found);
      })
      .catch((error: unknown) => setState({ kind: "failed", message: error instanceof Error ? error.message : String(error) }));
  };
  return (
    <div className="space-y-2 rounded-lg border border-line bg-panel p-3 text-sm">
      <form onSubmit={submit} role="search" className="flex gap-2">
        <input
          value={query}
          onChange={(event) => setQuery(event.target.value)}
          placeholder="Id, source:identifier or name"
          aria-label="Find an object by id, source:identifier or name"
          className="min-w-0 flex-1 rounded border border-line bg-space px-2 py-1"
        />
        <button type="submit" className="rounded border border-accent px-2 py-1 text-accent hover:bg-accent hover:text-space">
          Find
        </button>
      </form>
      <Outcome state={state} onFound={onFound} />
    </div>
  );
}

function Outcome({ state, onFound }: { state: State; onFound: (id: number) => void }) {
  switch (state.kind) {
    case "idle":
      return null;
    case "searching":
      return <p role="status" className="text-dim">Searching…</p>;
    case "failed":
      return <p role="alert" className="text-warn">{state.message}</p>;
    case "none":
      return <p role="status" className="text-warn">Nothing found for “{state.query}”.</p>;
    case "choices":
      return (
        <div>
          <p>“{state.query}” names {state.objects.length} objects:</p>
          <ul>
            {state.objects.map((o) => (
              <li key={o.id}>
                <button type="button" onClick={() => onFound(o.id)} className="text-accent hover:underline">
                  {o.id}
                  {o.names.length > 0 ? ` (${o.names.join(", ")})` : ""}
                </button>
              </li>
            ))}
          </ul>
        </div>
      );
  }
}
