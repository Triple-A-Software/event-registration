import ui from "@nuxt/ui/vue-plugin";
import { createApp } from "vue";
import { createRouter, createWebHashHistory } from "vue-router";
import "./style.css";
import App from "./App.vue";
import { i18n } from "./i18n";
import Event from "./pages/Event.vue";
import Events from "./pages/Events.vue";
import Settings from "./pages/Settings.vue";

const router = createRouter({
    history: createWebHashHistory(),
    routes: [
        { path: "/", component: Events },
        { path: "/events/:id", component: Event, props: (r) => ({ id: Number(r.params.id) }) },
        { path: "/settings", component: Settings },
    ],
});

createApp(App).use(router).use(ui).use(i18n).mount("#app");
