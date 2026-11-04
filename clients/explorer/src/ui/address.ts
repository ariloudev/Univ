// The page's address, which keeps what the view shows (read-an-object, choose-what-is-drawn).

/** Sets each of `params` in the page's address, removing those undefined, and leaves every other
 * parameter as it is. */
export function setInAddress(params: Readonly<Record<string, string | undefined>>): void {
  const url = new URL(location.href);
  for (const [name, value] of Object.entries(params)) {
    if (value === undefined) url.searchParams.delete(name);
    else url.searchParams.set(name, value);
  }
  history.replaceState(history.state, "", url);
}
