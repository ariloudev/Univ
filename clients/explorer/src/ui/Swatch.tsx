/** A dot of `colour`, beside the words that name what it marks; a ring of it where `hollow`, for
 * what is turned off; smaller where `small`, inside a chip. */
export function Swatch({ colour, hollow = false, small = false }: { colour: string; hollow?: boolean; small?: boolean }) {
  const style = hollow ? { boxShadow: `inset 0 0 0 1.5px ${colour}` } : { background: colour };
  return (
    <span aria-hidden="true" data-hollow={hollow ? "" : undefined} className={`inline-block shrink-0 rounded-full ${small ? "h-2.5 w-2.5" : "h-3 w-3"}`} style={style} />
  );
}
