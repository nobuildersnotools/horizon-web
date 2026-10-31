// Keeps the game's files for later visits. The large ones, the WebAssembly,
// the boot archive (their hashes from build.json) and the world generator's
// packs (from manifest.txt), are asked for with their content hash (`?v=…`)
// and come from the cache once stored, until a build changes them; the rest goes to the network first and falls back to the cache
// offline. Files the game fetches when it reads them (fs/res/) aren't kept.
// Its answers carry the headers that make the page cross-origin isolated (the
// game's threads share memory only then), for hosts that can't send them.
const CACHE = "horizon";

self.addEventListener("install", () => self.skipWaiting());
self.addEventListener("activate", (event) => event.waitUntil(self.clients.claim()));

self.addEventListener("fetch", (event) => {
    const url = new URL(event.request.url);
    if (event.request.method !== "GET" || url.origin !== location.origin || url.pathname.includes("/fs/res/")) {
        return;
    }
    event.respondWith((url.searchParams.has("v") ? hashed(event, url) : fresh(event)).then(isolated));
});

function isolated(response) {
    if (response.status === 0 || response.headers.get("Cross-Origin-Embedder-Policy")) return response;
    const headers = new Headers(response.headers);
    headers.set("Cross-Origin-Opener-Policy", "same-origin");
    headers.set("Cross-Origin-Embedder-Policy", "require-corp");
    return new Response(response.body, { status: response.status, statusText: response.statusText, headers });
}

async function hashed(event, url) {
    const cache = await caches.open(CACHE);
    const hit = await cache.match(event.request);
    if (hit) return hit;
    const response = await fetch(event.request);
    if (response.ok) {
        event.waitUntil((async () => {
            await cache.put(event.request, response.clone());
            // Other versions of this file are stale now.
            for (const old of await cache.keys()) {
                const at = new URL(old.url);
                if (at.pathname === url.pathname && at.search !== url.search) await cache.delete(old);
            }
        })());
    }
    return response;
}

async function fresh(event) {
    const cache = await caches.open(CACHE);
    try {
        const response = await fetch(event.request);
        if (response.ok) event.waitUntil(cache.put(event.request, response.clone()));
        return response;
    } catch (e) {
        const kept = await cache.match(event.request, { ignoreSearch: event.request.mode === "navigate" });
        if (kept) return kept;
        throw e;
    }
}
