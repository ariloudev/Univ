/** A dot of `colour`, beside the words that name what it marks; a ring of it where `hollow`, for
 * what is turned off. */
export function Swatch({ colour, hollow = false }: { colour: string; hollow?: boolean }) {
  const style = hollow ? { boxShadow: `inset 0 0 0 1.5px ${colour}` } : { background: colour };
  return <span aria-hidden="true" data-hollow={hollow ? "" : undefined} className="inline-block h-2.5 w-2.5 shrink-0 rounded-full" style={style} />;
}
