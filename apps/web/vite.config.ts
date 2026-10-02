import { defineConfig } from "vite"
import { tanstackStart } from "@tanstack/react-start/plugin/vite"
import viteReact from "@vitejs/plugin-react"
import tailwindcss from "@tailwindcss/vite"
import { nitro } from "nitro/vite"

const config = defineConfig({
  resolve: { tsconfigPaths: true },
  plugins: [
    tailwindcss(),
    tanstackStart(),
    nitro({
      routeRules: {
        "/**": {
          headers: {
            "x-frame-options": "DENY",
            "x-content-type-options": "nosniff",
            "referrer-policy": "no-referrer",
          },
        },
      },
    }),
    viteReact(),
  ],
})

export default config
