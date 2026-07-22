/**
 * Restos PWA — Service Worker
 *
 * Strategy:
 *  - App shell (HTML, WASM, JS, CSS): Cache-first with network fallback + background revalidation (stale-while-revalidate).
 *  - API calls (/api/*): Network-first, no caching — always fresh.
 *  - Static assets (/icons/*, /assets/*): Cache-first, long TTL.
 *
 * On activation the old caches are cleaned up so storage doesn't grow unbounded.
 */

const APP_VERSION = "v1";
const SHELL_CACHE = `restos-shell-${APP_VERSION}`;
const STATIC_CACHE = `restos-static-${APP_VERSION}`;
const ALL_CACHES = [SHELL_CACHE, STATIC_CACHE];

// Resources to pre-cache on install (app shell)
const SHELL_URLS = [
  "/",
  "/index.html",
];

// ── Install ─────────────────────────────────────────────────────────────────
self.addEventListener("install", (event) => {
  event.waitUntil(
    caches
      .open(SHELL_CACHE)
      .then((cache) => cache.addAll(SHELL_URLS))
      .then(() => self.skipWaiting())
  );
});

// ── Activate ─────────────────────────────────────────────────────────────────
self.addEventListener("activate", (event) => {
  event.waitUntil(
    caches
      .keys()
      .then((keys) =>
        Promise.all(
          keys
            .filter((k) => !ALL_CACHES.includes(k))
            .map((k) => caches.delete(k))
        )
      )
      .then(() => self.clients.claim())
  );
});

// ── Fetch ────────────────────────────────────────────────────────────────────
self.addEventListener("fetch", (event) => {
  const { request } = event;
  const url = new URL(request.url);

  // Skip non-GET and cross-origin requests
  if (request.method !== "GET" || url.origin !== self.location.origin) {
    return;
  }

  // API calls — always go to network, never cache
  if (url.pathname.startsWith("/api/")) {
    event.respondWith(fetch(request));
    return;
  }

  // Static assets — cache-first
  if (
    url.pathname.startsWith("/assets/") ||
    url.pathname.startsWith("/icons/")
  ) {
    event.respondWith(cacheFirst(request, STATIC_CACHE));
    return;
  }

  // Everything else (HTML, WASM, JS) — stale-while-revalidate
  event.respondWith(staleWhileRevalidate(request, SHELL_CACHE));
});

// ── Strategies ───────────────────────────────────────────────────────────────

async function cacheFirst(request, cacheName) {
  const cached = await caches.match(request);
  if (cached) return cached;
  const response = await fetch(request);
  if (response.ok) {
    const cache = await caches.open(cacheName);
    cache.put(request, response.clone());
  }
  return response;
}

async function staleWhileRevalidate(request, cacheName) {
  const cache = await caches.open(cacheName);
  const cached = await cache.match(request);

  const networkFetch = fetch(request).then((response) => {
    if (response.ok) {
      cache.put(request, response.clone());
    }
    return response;
  }).catch(() => null);

  // Return cached immediately, but refresh in background
  return cached || networkFetch;
}
