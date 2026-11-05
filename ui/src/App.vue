<script setup lang="ts">
import type { NavigationMenuItem } from "@nuxt/ui";
import { de, en } from "@nuxt/ui/locale";
import { computed } from "vue";
import { useI18n } from "vue-i18n";

const { t, locale } = useI18n();
const links = computed<NavigationMenuItem[]>(() => [
    { label: t("nav.events"), to: "/", icon: "i-lucide-calendar" },
    { label: t("nav.settings"), to: "/settings", icon: "i-lucide-settings" },
]);
</script>

<template>
    <UApp :locale="locale === 'de' ? de : en">
        <UContainer class="pb-12">
            <UNavigationMenu :items="links" class="mb-6" />
            <RouterView />
        </UContainer>
    </UApp>
</template>
