import { defineConfig } from "vite";

// Multi-página: el dashboard (index.html) + la atestación móvil (attest.html, F3).
export default defineConfig({
  build: {
    rollupOptions: {
      input: {
        main: "index.html",
        attest: "attest.html",
      },
    },
  },
});
