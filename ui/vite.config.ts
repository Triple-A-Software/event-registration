import ui from "@nuxt/ui/vite";
import vue from "@vitejs/plugin-vue";
import { defineConfig } from "vite";

export default defineConfig(({ command }) => {
    const production = command === "build";
    return {
        base: production ? "/api/rest/plugins/event-registration/ui" : undefined,
        define: {
            // neleto strips this prefix and forwards the rest, so calls use the full plugin.json route
            // (`/api/events` → `/api/rest/plugins/event-registration/api/api/events`); in dev they go straight to the plugin
            __API_BASE__: JSON.stringify(production ? "/api/rest/plugins/event-registration/api" : ""),
        },
        plugins: [
            vue(),
            ui({
                ui: {
                    colors: { primary: "orange", neutral: "zinc" },
                    formField: { slots: { root: "w-full" } },
                },
            }),
        ],
        resolve: {
            alias: { "@": "/src" },
        },
        server: {
            proxy: { "/api": "http://localhost:3000" },
        },
    };
});
