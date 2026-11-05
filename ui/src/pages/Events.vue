<script setup lang="ts">
import type { TableColumn } from "@nuxt/ui";
import { computed, onMounted, ref } from "vue";
import { useI18n } from "vue-i18n";
import { api, type EventOverview, eventStart, formatDate, useApi } from "@/api";

const { t, locale } = useI18n();
const call = useApi();
const events = ref<EventOverview[]>([]);
const loading = ref(true);
const showPast = ref(false);

onMounted(async () => {
    events.value = (await call(api<EventOverview[]>("/events"))) ?? [];
    loading.value = false;
});

const rows = computed(() => {
    const today = new Date();
    today.setHours(0, 0, 0, 0);
    return events.value.filter((e) => {
        const start = eventStart(e.event);
        return showPast.value || !start || start >= today;
    });
});

const columns = computed<TableColumn<EventOverview>[]>(() => [
    { id: "title", header: t("events.title") },
    { id: "date", header: t("events.date") },
    { id: "registration", header: t("events.registration") },
    { id: "seats", header: t("events.seats") },
    { id: "waitlist", header: t("events.waitlist") },
    { id: "actions", meta: { class: { td: "text-right" } } },
]);
</script>

<template>
    <div class="mb-4 flex flex-wrap items-center justify-between gap-4">
        <h1 class="text-xl font-semibold">{{ t("nav.events") }}</h1>
        <USwitch v-model="showPast" :label="t('events.showPast')" />
    </div>
    <UTable :data="rows" :columns="columns" :loading="loading" :empty="t('events.empty')">
        <template #title-cell="{ row }">
            <ULink :to="`/events/${row.original.id}`" class="font-medium text-highlighted">
                {{ row.original.event?.title ?? `#${row.original.id}` }}
            </ULink>
            <UBadge v-if="!row.original.event" color="error" variant="subtle" class="ml-2">{{ t("events.deleted") }}</UBadge>
            <UBadge v-else-if="!row.original.public" color="neutral" variant="subtle" class="ml-2">{{
                t("events.draft")
            }}</UBadge>
        </template>
        <template #date-cell="{ row }">{{ formatDate(eventStart(row.original.event), locale) }}</template>
        <template #registration-cell="{ row }">
            <UBadge :color="row.original.config?.enabled ? 'success' : 'neutral'" variant="subtle">
                {{ row.original.config?.enabled ? t("events.on") : t("events.off") }}
            </UBadge>
        </template>
        <template #seats-cell="{ row }">
            <template v-if="row.original.config">
                {{ row.original.confirmed_seats }}
                / {{ row.original.config.capacity ?? t("events.unlimited") }}
            </template>
        </template>
        <template #waitlist-cell="{ row }">{{ row.original.waitlisted_seats || "" }}</template>
        <template #actions-cell="{ row }">
            <UButton :to="`/events/${row.original.id}`" variant="ghost" icon="i-lucide-settings-2">{{
                t("events.manage")
            }}</UButton>
        </template>
    </UTable>
</template>
