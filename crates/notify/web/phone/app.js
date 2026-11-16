// 폰 화면: 알림 켜기(웹 푸시 구독), 승인 카드, 알림 목록, 하루 정리.
// 3D 사무실은 단추로 따로 연다(이 묶음은 3D를 불러오지 않는다).
'use strict';

const API = {
  pushKey: '/api/push/key',
  subscribe: '/api/push/subscribe',
  notices: '/api/notices',
  approval: (id) => '/api/approvals/' + encodeURIComponent(id),
  decide: (id, verb) => '/api/approvals/' + encodeURIComponent(id) + '/' + verb,
  dailySummary: '/api/daily-summary',
};
const OFFICE_PATH = '/';

const $ = (selector) => document.querySelector(selector);
let lastSeq = 0;

function base64UrlToBytes(text) {
  const pad = '='.repeat((4 - (text.length % 4)) % 4);
  const raw = atob((text + pad).replace(/-/g, '+').replace(/_/g, '/'));
  return Uint8Array.from(raw, (c) => c.charCodeAt(0));
}

async function getJson(path) {
  const response = await fetch(path, { credentials: 'same-origin' });
  if (!response.ok) throw new Error(String(response.status));
  return response.json();
}

async function postJson(path, body) {
  const response = await fetch(path, {
    method: 'POST',
    credentials: 'same-origin',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(body),
  });
  if (!response.ok) throw new Error(String(response.status));
  return response.status === 204 ? null : response.json().catch(() => null);
}

async function registerWorker() {
  if (!('serviceWorker' in navigator)) return null;
  return navigator.serviceWorker.register('sw.js', { scope: './' });
}

async function enablePush() {
  const status = $('[data-part="push-status"]');
  try {
    const registration = await registerWorker();
    if (!registration || !('pushManager' in registration)) {
      status.textContent = '이 브라우저는 알림을 받을 수 없어요. 홈 화면에 추가한 뒤 다시 해 보세요.';
      return;
    }
    const permission = await Notification.requestPermission();
    if (permission !== 'granted') {
      status.textContent = '알림 권한이 없어요.';
      return;
    }
    const { key } = await getJson(API.pushKey);
    const subscription = await registration.pushManager.subscribe({
      userVisibleOnly: true,
      applicationServerKey: base64UrlToBytes(key),
    });
    const json = subscription.toJSON();
    await postJson(API.subscribe, {
      endpoint: json.endpoint,
      p256dh: json.keys.p256dh,
      auth: json.keys.auth,
      device: navigator.userAgent.slice(0, 60),
    });
    status.textContent = '알림을 켰어요.';
  } catch (error) {
    status.textContent = '알림을 켜지 못했어요: ' + error.message;
  }
}

function addNotice(notice) {
  const item = document.createElement('li');
  item.dataset.seq = String(notice.seq);
  const title = document.createElement('strong');
  title.textContent = notice.title;
  const body = document.createElement('div');
  body.className = 'muted';
  body.textContent = notice.body;
  item.append(title, body);
  if (notice.link) {
    item.addEventListener('click', () => openLink(notice.link));
  }
  $('#notice-list').prepend(item);
}

async function loadNotices() {
  try {
    const list = await getJson(API.notices + '?after=' + lastSeq + '&max=50');
    for (const notice of list) {
      addNotice(notice);
      lastSeq = Math.max(lastSeq, notice.seq);
    }
  } catch (_) {
    // 서버가 잠시 안 되면 다음에 다시 읽는다.
  }
}

async function showApproval(id) {
  const card = $('[data-card="approval"]');
  try {
    const approval = await getJson(API.approval(id));
    card.dataset.approvalId = id;
    card.dataset.fingerprint = approval.fingerprint || '';
    card.querySelector('[data-part="summary"]').textContent = approval.summary || '';
    card.querySelector('[data-part="fingerprint"]').textContent = approval.fingerprint || '';
    card.querySelector('[data-part="result"]').textContent = '';
    // 결정 4번: 지문 앞 6자리를 보이기만 한다. 사용자가 치지 않는다.
    card.querySelector('[data-field="code"]').value = (approval.fingerprint || '').slice(0, 6);
    card.hidden = false;
  } catch (error) {
    card.hidden = true;
  }
}

async function decide(verb) {
  const card = $('[data-card="approval"]');
  const result = card.querySelector('[data-part="result"]');
  if (!card.dataset.approvalId || !card.dataset.fingerprint) {
    result.textContent = '승인 내용을 아직 받지 못했어요.';
    return;
  }
  try {
    // 화면이 받은 승인 카드의 지문을 그대로 보낸다(서버가 지금 내용의 지문과 비교한다).
    await postJson(API.decide(card.dataset.approvalId, verb), {
      fingerprint: card.dataset.fingerprint,
    });
    result.textContent = verb === 'approve' ? '승인했어요.' : '거절했어요.';
  } catch (error) {
    result.textContent = '처리하지 못했어요: ' + error.message;
  }
}

async function loadDailySummary() {
  try {
    const summary = await getJson(API.dailySummary);
    for (const part of ['done', 'pnl', 'review']) {
      const box = $('#daily-summary [data-part="' + part + '"] p');
      if (box && summary[part]) box.textContent = summary[part];
    }
  } catch (_) {
    // 아직 하루 정리가 없다.
  }
}

function openLink(link) {
  const match = /\/approvals\/([^/?#]+)/.exec(link);
  if (match) showApproval(decodeURIComponent(match[1]));
}

document.addEventListener('click', (event) => {
  const action = event.target.closest('[data-action]');
  if (!action) return;
  switch (action.dataset.action) {
    case 'enable-push': enablePush(); break;
    case 'approve': decide('approve'); break;
    case 'reject': decide('reject'); break;
    case 'open-office': window.location.href = OFFICE_PATH; break;
    default: break;
  }
});

if ('serviceWorker' in navigator) {
  navigator.serviceWorker.addEventListener('message', (event) => {
    const message = event.data || {};
    if (message.type === 'notice') loadNotices();
    if (message.type === 'open' && message.link) openLink(new URL(message.link).pathname);
  });
}

registerWorker().catch(() => {});
openLink(window.location.pathname + window.location.hash.replace('#', ''));
loadNotices();
loadDailySummary();
