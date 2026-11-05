<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import { useI18n } from "vue-i18n";
import { api, type Settings, useApi } from "@/api";

const { t } = useI18n();
const call = useApi();
const toast = useToast();
const settings = ref<Settings>({});
const saving = ref(false);

onMounted(async () => {
    const s = await call(api<Settings>("/settings"));
    if (s) settings.value = s;
});

// the select can't hold null
const provider = computed({
    get: () => settings.value.captcha_provider ?? "none",
    set: (v) => (settings.value.captcha_provider = v === "none" ? undefined : v),
});
const providers = computed(() => [
    { label: t("settings.none"), value: "none" as const },
    { label: "hCaptcha", value: "h-captcha" as const },
    { label: "reCAPTCHA", value: "re-captcha" as const },
    { label: "Cloudflare Turnstile", value: "turnstile" as const },
]);

async function save() {
    saving.value = true;
    const s = await call(api<Settings>("/settings", "PUT", settings.value));
    saving.value = false;
    if (!s) return;
    settings.value = s;
    toast.add({ title: t("settings.saved"), color: "success", icon: "i-lucide-check" });
}
</script>

<template>
    <h1 class="mb-4 text-xl font-semibold">{{ t("settings.title") }}</h1>
    <UCard>
        <form class="grid max-w-xl gap-4" @submit.prevent="save">
            <UFormField :label="t('settings.notifyEmail')" :description="t('settings.notifyEmailHint')">
                <UInput v-model="settings.notify_email" type="email" class="w-full" />
            </UFormField>
            <UFormField :label="t('settings.captcha')">
                <USelect v-model="provider" :items="providers" class="w-full" />
            </UFormField>
            <template v-if="settings.captcha_provider">
                <UFormField :label="t('settings.sitekey')">
                    <UInput v-model="settings.captcha_sitekey" class="w-full" />
                </UFormField>
                <UFormField :label="t('settings.secret')">
                    <UInput v-model="settings.captcha_secret" type="password" class="w-full" />
                </UFormField>
            </template>
            <div>
                <UButton type="submit" :loading="saving" icon="i-lucide-save">{{ t("save") }}</UButton>
            </div>
        </form>
    </UCard>
</template>
