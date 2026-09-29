// Argus's service worker: shows a push when an agent needs someone, and
// opens that agent when the push is tapped. It caches nothing, so the page
// is always the one this argus web serves.

self.addEventListener("push", (event) => {
  let notice = {};
  try {
    notice = event.data ? event.data.json() : {};
  } catch {
    notice = {};
  }
  event.waitUntil(
    self.registration.showNotification(notice.title || "Argus", {
      body: notice.body || "",
      tag: notice.pane !== undefined ? `pane-${notice.pane}` : undefined,
      renotify: true,
      icon: "/icon-192.png",
      data: { url: notice.url || "/" },
    }),
  );
});

self.addEventListener("notificationclick", (event) => {
  event.notification.close();
  const url = new URL(event.notification.data?.url || "/", self.location.origin).href;
  event.waitUntil(
    self.clients.matchAll({ type: "window", includeUncontrolled: true }).then((windows) => {
      for (const client of windows) {
        if ("focus" in client) {
          client.navigate(url);
          return client.focus();
        }
      }
      return self.clients.openWindow(url);
    }),
  );
});
