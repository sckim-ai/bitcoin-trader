const CACHE_NAME = 'btc-trader-v1';

self.addEventListener('install', () => self.skipWaiting());
self.addEventListener('activate', e => e.waitUntil(self.clients.claim()));

self.addEventListener('fetch', e => {
  const req = e.request;
  // Skip navigation and non-GET requests entirely — let the browser handle them.
  if (req.mode === 'navigate' || req.method !== 'GET') return;
  // Only cache same-origin static assets.
  const url = new URL(req.url);
  if (url.origin !== self.location.origin) return;

  e.respondWith(
    caches.match(req).then(cached => cached || fetch(req).catch(() => cached))
  );
});
