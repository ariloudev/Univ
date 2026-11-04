/** The command that starts Univ Server and the explorer together. */
export const START_COMMAND = "cargo xtask explore";

/** Univ Server is not answering: where the explorer looked, and how to start it. */
export function Unreachable({ address, onRetry }: { address: string; onRetry: () => void }) {
  return (
    <main className="mx-auto mt-24 max-w-xl space-y-4 rounded-lg border border-line bg-panel p-6">
      <h1 className="text-xl font-semibold">Univ Server is not answering</h1>
      <p>
        The explorer tried <code className="text-accent">{address}</code> and got no answer.
      </p>
      <p>
        Start Univ Server and the explorer together from the <code>univ-server</code> directory:
      </p>
      <pre className="rounded bg-space p-3 text-accent">{START_COMMAND}</pre>
      <button
        type="button"
        onClick={onRetry}
        className="rounded border border-accent px-3 py-1 text-accent hover:bg-accent hover:text-space"
      >
        Try again
      </button>
    </main>
  );
}
