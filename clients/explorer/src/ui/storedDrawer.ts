// The drawer the rail has open, kept in the browser's local storage, so that the explorer opens
// with the drawer the person last left open there (frame-the-view-as-an-atlas).

import { useState } from "react";

import { type DrawerName, isDrawerName } from "./Rail.tsx";

/** The local storage key that keeps the open drawer. */
export const DRAWER_KEY = "univ-explorer.drawer";

/** The drawer `storage` keeps, if it keeps one the rail still opens; none where the browser
 * refuses its storage. */
export function storedDrawer(storage: () => Storage): DrawerName | undefined {
  try {
    const kept = storage().getItem(DRAWER_KEY);
    return isDrawerName(kept) ? kept : undefined;
  } catch {
    return undefined;
  }
}

/** Keeps `drawer` in `storage`, or that none is open; nothing where the browser refuses it. */
export function storeDrawer(storage: () => Storage, drawer: DrawerName | undefined): void {
  try {
    if (drawer === undefined) storage().removeItem(DRAWER_KEY);
    else storage().setItem(DRAWER_KEY, drawer);
  } catch {
    // The drawer is not kept, and opens as ever.
  }
}

const local = () => localStorage;

/** The drawer open, first the one local storage keeps, and the way to open another or none,
 * kept there. */
export function useStoredDrawer(): [DrawerName | undefined, (drawer: DrawerName | undefined) => void] {
  const [drawer, setDrawer] = useState(() => storedDrawer(local));
  return [
    drawer,
    (next) => {
      setDrawer(next);
      storeDrawer(local, next);
    },
  ];
}
