// The drawer the rail has open, kept in the browser's local storage, so that the explorer opens
// with the drawer the person last left open there (frame-the-view-as-an-atlas).

import { useLocalState } from "./localState.ts";
import { type DrawerName, isDrawerName } from "./Rail.tsx";

/** The local storage key that keeps the open drawer. */
export const DRAWER_KEY = "univ-explorer.drawer";

/** The drawer kept, if it is one the rail still opens. */
const drawerOf = (kept: string | undefined): DrawerName | undefined => (isDrawerName(kept) ? kept : undefined);
const keptDrawer = (drawer: DrawerName | undefined): string | undefined => drawer;

/** The drawer open, first the one local storage keeps, and the way to open another or none,
 * kept there. */
export const useStoredDrawer = (): [DrawerName | undefined, (drawer: DrawerName | undefined) => void] => useLocalState(DRAWER_KEY, drawerOf, keptDrawer);
