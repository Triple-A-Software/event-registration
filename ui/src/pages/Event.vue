<script setup lang="ts">
import type { TableColumn } from "@nuxt/ui";
import { computed, onMounted, ref, watch } from "vue";
import { useI18n } from "vue-i18n";
import {
    api,
    apiUrl,
    type EventConfig,
    type EventOverview,
    eventStart,
    formatDate,
    type Registration,
    type Status,
    useApi,
} from "@/api";

const props = defineProps<{ id: number }>();
const { t, locale } = useI18n();
const call = useApi();
const toast = useToast();

const overview = ref<EventOverview>();
const config = ref<EventConfig>({
    enabled: false,
    max_seats_per_registration: 1,
    waitlist_enabled: false,
});
const saving = ref(false);

/** `<input type="datetime-local">` works in the browser's time zone, the api in UTC. */
function toInput(iso?: string): string {
    if (!iso) return "";
    const d = new Date(iso);
    const p = (n: number) => String(n).padStart(2, "0");
    return `${d.getFullYear()}-${p(d.getMonth() + 1)}-${p(d.getDate())}T${p(d.getHours())}:${p(d.getMinutes())}`;
}
const opensAt = computed({
    get: () => toInput(config.value.opens_at),
    set: (v: string) => (config.value.opens_at = v ? new Date(v).toISOString() : undefined),
});
const closesAt = computed({
    get: () => toInput(config.value.closes_at),
    set: (v: string) => (config.value.closes_at = v ? new Date(v).toISOString() : undefined),
});

function show(o: EventOverview) {
    overview.value = o;
    if (o.config) config.value = { ...o.config };
}

async function save() {
    saving.value = true;
    const o = await call(api<EventOverview>(`/events/${props.id}`, "PUT", config.value));
    saving.value = false;
    if (!o) return;
    show(o);
    toast.add({ title: t("config.saved"), color: "success", icon: "i-lucide-check" });
    // a raised capacity may have moved people up from the waitlist
    await loadRegistrations();
}

const registrations = ref<Registration[]>([]);
const loadingRegistrations = ref(true);
const status = ref<Status | "all">("all");
const search = ref("");

async function loadRegistrations() {
    const params = new URLSearchParams({ event_id: String(props.id) });
    if (status.value !== "all") params.set("status", status.value);
    if (search.value.trim()) params.set("q", search.value.trim());
    registrations.value = (await call(api<Registration[]>(`/registrations?${params}`))) ?? [];
    loadingRegistrations.value = false;
}

let searchTimer: ReturnType<typeof setTimeout> | undefined;
watch(status, loadRegistrations);
watch(search, () => {
    clearTimeout(searchTimer);
    searchTimer = setTimeout(loadRegistrations, 250);
});

onMounted(async () => {
    const o = await call(api<EventOverview>(`/events/${props.id}`));
    if (o) show(o);
    await loadRegistrations();
});

async function refresh() {
    await loadRegistrations();
    const o = await call(api<EventOverview>(`/events/${props.id}`));
    if (o) overview.value = o;
}

async function setStatus(r: Registration, s: Status) {
    if (s === r.status) return;
    await call(api(`/registrations`, "PUT", { id: r.id, status: s }));
    await refresh();
}

const toDelete = ref<Registration>();
const deleteOpen = computed({
    get: () => toDelete.value !== undefined,
    set: (v) => {
        if (!v) toDelete.value = undefined;
    },
});
async function remove() {
    if (!toDelete.value) return;
    await call(api(`/registrations?id=${toDelete.value.id}`, "DELETE"));
    toDelete.value = undefined;
    await refresh();
}

const statusItems = computed(() =>
    (["confirmed", "waitlisted", "cancelled"] as const).map((s) => ({ label: t(`status.${s}`), value: s })),
);
const filterItems = computed(() => [{ label: t("registrations.all"), value: "all" as const }, ...statusItems.value]);
const columns = computed<TableColumn<Registration>[]>(() => [
    { id: "name", header: t("registrations.name") },
    { id: "seats", header: t("registrations.seats") },
    { id: "status", header: t("registrations.status") },
    { id: "comment", header: t("registrations.comment") },
    { id: "created", header: t("registrations.createdAt") },
    { id: "actions", meta: { class: { td: "text-right" } } },
]);
</script>

<template>
    <UButton to="/" variant="link" icon="i-lucide-arrow-left" class="mb-2 px-0">{{ t("back") }}</UButton>
    <div class="mb-6">
        <h1 class="text-xl font-semibold">{{ overview?.event?.title ?? t("events.deleted") }}</h1>
        <p v-if="overview" class="text-muted">
            {{ formatDate(eventStart(overview.event), locale) }} ·
            {{ overview.confirmed_seats }} / {{ overview.config?.capacity ?? t("events.unlimited") }} {{ t("events.seats") }}
            <template v-if="overview.waitlisted_seats"> · {{ overview.waitlisted_seats }} {{ t("events.waitlist") }}</template>
        </p>
    </div>

    <UCard class="mb-8">
        <template #header>
            <h2 class="font-semibold">{{ t("config.title") }}</h2>
        </template>
        <form class="grid gap-4 sm:grid-cols-2" @submit.prevent="save">
            <USwitch v-model="config.enabled" :label="t('config.enabled')" class="sm:col-span-2" />
            <UFormField :label="t('config.capacity')" :hint="t('config.capacityHint')">
                <UInputNumber v-model="config.capacity" :min="1" class="w-full" />
            </UFormField>
            <UFormField :label="t('config.maxSeats')">
                <UInputNumber v-model="config.max_seats_per_registration" :min="1" class="w-full" />
            </UFormField>
            <USwitch v-model="config.waitlist_enabled" :label="t('config.waitlist')" class="sm:col-span-2" />
            <UFormField :label="t('config.opensAt')" :hint="t('config.opensAtHint')">
                <UInput v-model="opensAt" type="datetime-local" class="w-full" />
            </UFormField>
            <UFormField :label="t('config.closesAt')" :hint="t('config.closesAtHint')">
                <UInput v-model="closesAt" type="datetime-local" class="w-full" />
            </UFormField>
            <UFormField :label="t('config.notifyEmail')" :hint="t('config.notifyEmailHint')" class="sm:col-span-2">
                <UInput v-model="config.notify_email" type="email" class="w-full" />
            </UFormField>
            <div class="sm:col-span-2">
                <UButton type="submit" :loading="saving" icon="i-lucide-save">{{ t("save") }}</UButton>
            </div>
        </form>
    </UCard>

    <div class="mb-4 flex flex-wrap items-center gap-3">
        <h2 class="mr-auto text-lg font-semibold">{{ t("registrations.title") }}</h2>
        <UInput v-model="search" :placeholder="t('registrations.search')" icon="i-lucide-search" />
        <USelect v-model="status" :items="filterItems" class="w-40" />
        <UButton
            :to="apiUrl(`/registrations/export?event_id=${id}&lang=${locale}`)"
            external
            variant="outline"
            icon="i-lucide-download"
        >
            {{ t("registrations.export") }}
        </UButton>
    </div>
    <UTable :data="registrations" :columns="columns" :loading="loadingRegistrations" :empty="t('registrations.empty')">
        <template #name-cell="{ row }">
            <div class="font-medium text-highlighted">{{ row.original.name }}</div>
            <div class="text-muted">{{ row.original.email }}</div>
            <div v-if="row.original.phone" class="text-muted">{{ row.original.phone }}</div>
        </template>
        <template #seats-cell="{ row }">{{ row.original.seats }}</template>
        <template #status-cell="{ row }">
            <USelect
                :model-value="row.original.status"
                :items="statusItems"
                class="w-36"
                @update:model-value="(s: unknown) => setStatus(row.original, s as Status)"
            />
        </template>
        <template #comment-cell="{ row }">
            <span class="whitespace-pre-line">{{ row.original.comment }}</span>
        </template>
        <template #created-cell="{ row }">{{ formatDate(row.original.created_at, locale) }}</template>
        <template #actions-cell="{ row }">
            <UButton
                color="error"
                variant="ghost"
                icon="i-lucide-trash-2"
                :aria-label="t('delete')"
                @click="toDelete = row.original"
            />
        </template>
    </UTable>

    <UModal v-model:open="deleteOpen" :title="t('registrations.deleteTitle')">
        <template #body>{{ t("registrations.deleteText", { name: toDelete?.name }) }}</template>
        <template #footer>
            <div class="flex w-full justify-end gap-2">
                <UButton variant="ghost" @click="toDelete = undefined">{{ t("cancel") }}</UButton>
                <UButton color="error" icon="i-lucide-trash-2" @click="remove">{{ t("delete") }}</UButton>
            </div>
        </template>
    </UModal>
</template>
