/** The halo behind the demo's window: a slow, breathing gradient. */
export function Glow() {
  return (
    <div
      aria-hidden="true"
      className="demo-glow pointer-events-none absolute inset-x-[5%] inset-y-[2%] -z-10 bg-linear-to-r from-primary via-sky-400 to-violet-500 bg-size-[200%_100%] blur-lg sm:blur-2xl"
    />
  )
}
