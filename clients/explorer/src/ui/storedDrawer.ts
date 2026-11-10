// The drawer the rail has open, kept in the browser's local storage, so that the explorer opens
// with the drawer the person last left open there (frame-the-view-as-an-atlas).

import { readLocal, useLocalState, writeLocal } from "./localState.ts";
import { type DrawerName, isDrawerName } from "./Rail.tsx";

/** The local storage key that keeps the open drawer. */
export const DRAWER_KEY = "univ-explorer.drawer";

/** The drawer kept, if it is one the rail still opens. */
const drawerOf = (kept: string | undefined): DrawerName | undefined => (isDrawerName(kept) ? kept : undefined);

/** The drawer `storage` keeps, if it keeps one the rail still opens; none where the browser
 * refuses its storage. */
export const storedDrawer = (storage: () => Storage): DrawerName | undefined => drawerOf(readLocal(DRAWER_KEY, storage));

/** Keeps `drawer` in `storage`, or that none is open; nothing where the browser refuses it. */
export const storeDrawer = (storage: () => Storage, drawer: DrawerName | undefined): void => writeLocal(DRAWER_KEY, drawer, storage);

/** The drawer open, first the one local storage keeps, and the way to open another or none,
 * kept there. */
export const useStoredDrawer = (): [DrawerName | undefined, (drawer: DrawerName | undefined) => void] => useLocalState(DRAWER_KEY, drawerOf, (drawer) => drawer);
