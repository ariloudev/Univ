/** The command that starts Univ Server and the explorer together. */
export const START_COMMAND = "cargo xtask explore";

/** Univ Server is not answering: where the explorer looked, and how to start it. */
export function ServerUnreachable({ address, onRetry }: { address: string; onRetry: () => void }) {
  return (
    <main className="mx-auto mt-24 max-w-xl space-y-4 rounded-lg border border-line bg-panel p-6">
      <h1 className="text-xl font-semibold">Univ Server is not answering</h1>
      <p>
        Nothing answered at <code className="text-accent">{address}</code>.
      </p>
      <p>
        Start Univ Server and the explorer together from the <code>univ-server</code> directory:
      </p>
      <pre className="rounded bg-space p-3 text-accent">{START_COMMAND}</pre>
      <TryAgain onRetry={onRetry} />
    </main>
  );
}

/** Reads the releases anew, without reloading the page. */
export function TryAgain({ onRetry }: { onRetry: () => void }) {
  return (
    <button
      type="button"
      onClick={onRetry}
      className="rounded border border-accent px-3 py-1 text-accent hover:bg-accent hover:text-space"
    >
      Try again
    </button>
  );
}
