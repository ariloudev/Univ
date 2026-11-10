// A value of the page kept in the browser's local storage, so that the explorer opens as the
// person last left it in that browser: which drawer is open (frame-the-view-as-an-atlas), and the
// depth index's order and find text (see-the-structures). Where the browser refuses its storage,
// nothing is kept and the page opens as ever.

import { useCallback, useState } from "react";

const local = (): Storage => localStorage;

/** What `storage` keeps under `key`; none where it keeps nothing or the browser refuses it. */
export function readLocal(key: string, storage: () => Storage = local): string | undefined {
  try {
    return storage().getItem(key) ?? undefined;
  } catch {
    return undefined;
  }
}

/** Keeps `value` in `storage` under `key`, or removes the key for none; nothing where the
 * browser refuses it. */
export function writeLocal(key: string, value: string | undefined, storage: () => Storage = local): void {
  try {
    if (value === undefined) storage().removeItem(key);
    else storage().setItem(key, value);
  } catch {
    // Not kept: the page opens as ever next time.
  }
}

/** A value kept in local storage under `key`: first what `read` makes of what is kept, then each
 * value set, kept as `write` gives it (none to keep nothing). */
export function useLocalState<T>(key: string, read: (kept: string | undefined) => T, write: (value: T) => string | undefined): [T, (value: T) => void] {
  const [value, setValue] = useState(() => read(readLocal(key)));
  // One setter for the hook's life, as useState's own is.
  const set = useCallback(
    (next: T) => {
      setValue(next);
      writeLocal(key, write(next));
    },
    [key, write],
  );
  return [value, set];
}
