// 서비스 워커: 푸시를 받아 알림을 띄우고, 누르면 폰 화면을 연다.
'use strict';

self.addEventListener('install', (event) => {
  event.waitUntil(self.skipWaiting());
});

self.addEventListener('activate', (event) => {
  event.waitUntil(self.clients.claim());
});

self.addEventListener('push', (event) => {
  let data = {};
  try {
    data = event.data ? event.data.json() : {};
  } catch (_) {
    data = { title: '가격인하', body: event.data ? event.data.text() : '' };
  }
  const title = data.title || '가격인하';
  const options = {
    body: data.body || '',
    icon: 'icon-192.png',
    badge: 'icon-192.png',
    tag: data.kind ? data.kind + '-' + String(data.seq || '') : undefined,
    requireInteraction: data.kind === 'approval_request',
    data: { link: data.link || './', seq: data.seq, kind: data.kind },
  };
  event.waitUntil(
    self.registration.showNotification(title, options).then(() => broadcast(data)),
  );
});

self.addEventListener('notificationclick', (event) => {
  event.notification.close();
  const link = (event.notification.data && event.notification.data.link) || './';
  const target = new URL(link, self.registration.scope).href;
  event.waitUntil(
    self.clients.matchAll({ type: 'window', includeUncontrolled: true }).then((list) => {
      for (const client of list) {
        if ('focus' in client) {
          client.postMessage({ type: 'open', link: target });
          return client.focus();
        }
      }
      return self.clients.openWindow(target);
    }),
  );
});

function broadcast(data) {
  return self.clients
    .matchAll({ type: 'window', includeUncontrolled: true })
    .then((list) => list.forEach((client) => client.postMessage({ type: 'notice', notice: data })));
}
