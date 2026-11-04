/** A dot of `colour`, beside the words that name what it marks. */
export function Swatch({ colour }: { colour: string }) {
  return <span aria-hidden="true" className="inline-block h-3 w-3 shrink-0 rounded-full" style={{ background: colour }} />;
}
