/** Types of the plugin's JSON, mirroring `src/model.rs`. */
export type Status = "confirmed" | "waitlisted" | "cancelled";

export interface CmsEvent {
    id: number;
    slug: string;
    title: string;
    /** UTC without zone, as the CMS stores it */
    start?: string;
    end?: string;
}

export interface EventConfig {
    enabled: boolean;
    capacity?: number;
    max_seats_per_registration: number;
    waitlist_enabled: boolean;
    opens_at?: string;
    closes_at?: string;
    notify_email?: string;
}

export interface EventOverview {
    id: number;
    /** missing = deleted in the CMS */
    event?: CmsEvent;
    public: boolean;
    config?: EventConfig;
    confirmed_seats: number;
    waitlisted_seats: number;
    active_registrations: number;
}

export interface Registration {
    id: string;
    event_id: number;
    name: string;
    email: string;
    phone?: string;
    seats: number;
    comment?: string;
    status: Status;
    created_at: string;
    cancelled_at?: string;
    language: string;
}

export interface Settings {
    notify_email?: string;
    captcha_provider?: "h-captcha" | "re-captcha" | "turnstile";
    captcha_sitekey?: string;
    captcha_secret?: string;
}

export const apiUrl = (path: string) => __API_BASE__ + path;

/** Throws the plugin's `{error}` message on a non-2xx response. */
export async function api<T>(path: string, method = "GET", body?: unknown): Promise<T> {
    const res = await fetch(apiUrl(path), {
        method,
        headers: body === undefined ? {} : { "content-type": "application/json" },
        body: body === undefined ? undefined : JSON.stringify(body),
    });
    // null → undefined, like form-plugin: Nuxt UI inputs don't take null, and serde reads a
    // missing Option field as None
    const json = await res
        .text()
        .then((t) => JSON.parse(t, (_, v) => (v === null ? undefined : v)))
        .catch(() => undefined);
    if (!res.ok) throw new Error(json?.error ?? `${res.status} ${res.statusText}`.trim());
    return json as T;
}

/** The CMS event start is UTC without a zone marker. */
export function eventStart(e?: CmsEvent): Date | undefined {
    return e?.start ? new Date(`${e.start}Z`) : undefined;
}

export function formatDate(d: Date | string | undefined, locale: string): string {
    if (!d) return "–";
    return new Date(d).toLocaleString(locale, { dateStyle: "medium", timeStyle: "short" });
}

/** Runs an api call and shows its error as a toast. Resolves to undefined on error. */
export function useApi() {
    const toast = useToast();
    return async <T>(call: Promise<T>): Promise<T | undefined> => {
        try {
            return await call;
        } catch (e) {
            toast.add({ title: (e as Error).message, color: "error", icon: "i-lucide-circle-alert" });
        }
    };
}
