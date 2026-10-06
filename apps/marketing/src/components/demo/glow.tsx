/** The halo behind the demo's window, in the text colour, slowly breathing. */
export function Glow() {
  return (
    <div
      aria-hidden="true"
      className="demo-glow pointer-events-none absolute inset-x-[5%] inset-y-[2%] -z-10 bg-foreground blur-lg sm:blur-2xl"
    />
  )
}
