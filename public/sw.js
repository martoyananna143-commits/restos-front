/**
 * RestOS PWA service worker.
 *
 * The release builder replaces the placeholder below with the SHA-256 release
 * identifier derived from the emitted HTML/JS/WASM/CSS artifact manifest.
 */

const APP_VERSION = "__RESTOS_RELEASE_ID__";
const RELEASE_ID_PATTERN = /^[a-f0-9]{64}$/;

if (!RELEASE_ID_PATTERN.test(APP_VERSION)) {
  throw new Error("RestOS service worker release ID is unavailable");
}

const SHELL_CACHE = `restos-shell-${APP_VERSION}`;
const STATIC_CACHE = `restos-static-${APP_VERSION}`;
const CURRENT_CACHES = [SHELL_CACHE, STATIC_CACHE];
const RESTOS_CACHE_PREFIXES = ["restos-shell-", "restos-static-"];
const SHELL_URLS = ["/", "/index.html"];

self.addEventListener("install", (event) => {
  event.waitUntil(
    (async () => {
      const cache = await caches.open(SHELL_CACHE);
      for (const path of SHELL_URLS) {
        const response = await fetch(new Request(path, { cache: "reload" }));
        if (!response.ok) {
          throw new Error("RestOS shell installation failed");
        }
        await cache.put(path, response.clone());
      }
      await self.skipWaiting();
    })()
  );
});

self.addEventListener("activate", (event) => {
  event.waitUntil(
    (async () => {
      const keys = await caches.keys();
      await Promise.all(
        keys
          .filter(
            (key) =>
              RESTOS_CACHE_PREFIXES.some((prefix) => key.startsWith(prefix)) &&
              !CURRENT_CACHES.includes(key)
          )
          .map((key) => caches.delete(key))
      );
      await self.clients.claim();
    })()
  );
});

self.addEventListener("fetch", (event) => {
  const { request } = event;
  const url = new URL(request.url);

  if (request.method !== "GET" || url.origin !== self.location.origin) {
    return;
  }

  if (url.pathname.startsWith("/api/")) {
    event.respondWith(fetch(request));
    return;
  }

  if (url.pathname === "/sw.js") {
    event.respondWith(fetch(request, { cache: "no-store" }));
    return;
  }

  if (
    request.mode === "navigate" ||
    url.pathname === "/" ||
    url.pathname === "/index.html"
  ) {
    event.respondWith(networkFirstShell(request));
    return;
  }

  if (
    url.pathname.startsWith("/assets/") ||
    url.pathname.startsWith("/icons/") ||
    url.pathname.startsWith("/brand/") ||
    url.pathname.startsWith("/favicon")
  ) {
    event.respondWith(cacheFirst(request, STATIC_CACHE));
  }
});

async function networkFirstShell(request) {
  const cache = await caches.open(SHELL_CACHE);
  try {
    const response = await fetch(request, { cache: "no-store" });
    if (response.ok) {
      await cache.put(request, response.clone());
    }
    return response;
  } catch (_error) {
    const cached =
      (await cache.match(request, { ignoreSearch: true })) ||
      (await cache.match("/index.html")) ||
      (await cache.match("/"));
    return cached || controlledOfflineResponse();
  }
}

async function cacheFirst(request, cacheName) {
  const cache = await caches.open(cacheName);
  const cached = await cache.match(request);
  if (cached) {
    return cached;
  }
  const response = await fetch(request);
  if (response.ok) {
    await cache.put(request, response.clone());
  }
  return response;
}

function controlledOfflineResponse() {
  return new Response(
    "<!doctype html><html lang=\"ru\"><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width,initial-scale=1\"><title>RestOS — нет связи</title><main><h1>Нет связи с RestOS</h1><p>Проверьте интернет-соединение и обновите страницу.</p></main></html>",
    {
      status: 503,
      headers: {
        "Content-Type": "text/html; charset=utf-8",
        "Cache-Control": "no-store",
      },
    }
  );
}
