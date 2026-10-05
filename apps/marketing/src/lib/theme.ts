/** Runs in <head> before the page is drawn: dark unless the theme cookie says light. */
export const THEME_SCRIPT = `document.documentElement.classList.toggle("dark", !document.cookie.includes("theme=light"))`

/** Switches between dark and light, for this site and the web app, which share the cookie. */
export function toggleTheme() {
  const dark = document.documentElement.classList.toggle("dark")
  const domain = location.hostname.endsWith("motile.app")
    ? "; domain=motile.app"
    : ""
  document.cookie = `theme=${dark ? "dark" : "light"}; path=/; max-age=31536000; samesite=lax${domain}`
}
