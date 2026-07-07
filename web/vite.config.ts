import { defineConfig } from "vite";

// Multi-página: landing (index), onboarding, dashboard y atestación móvil (F3).
export default defineConfig({
  build: {
    rollupOptions: {
      input: {
        main: "index.html",
        onboarding: "onboarding.html",
        dashboard: "dashboard.html",
        attest: "attest.html",
      },
    },
  },
});
