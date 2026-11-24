// 폰 화면: 알림 켜기(웹 푸시 구독), 승인 카드, 알림 목록, 하루 정리.
// 3D 사무실은 단추로 따로 연다(이 묶음은 3D를 불러오지 않는다).
import "./phone.css";
import { API, SCREEN_ROOTS, approvalPath, decidePath, type Verb } from "../paths";

const OFFICE_PATH = SCREEN_ROOTS.office;

interface Notice {
  seq: number;
  title: string;
  body: string;
  link?: string;
}

const $ = <T extends HTMLElement = HTMLElement>(selector: string): T => document.querySelector(selector) as T;
let lastSeq = 0;

function message(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

function base64UrlToBytes(text: string): Uint8Array<ArrayBuffer> {
  const pad = "=".repeat((4 - (text.length % 4)) % 4);
  const raw = atob((text + pad).replace(/-/g, "+").replace(/_/g, "/"));
  return Uint8Array.from(raw, (c) => c.charCodeAt(0));
}

async function getJson<T>(path: string): Promise<T> {
  const response = await fetch(path, { credentials: "same-origin" });
  if (!response.ok) throw new Error(String(response.status));
  return response.json() as Promise<T>;
}

async function postJson(path: string, body: unknown): Promise<unknown> {
  const response = await fetch(path, {
    method: "POST",
    credentials: "same-origin",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(body),
  });
  if (!response.ok) throw new Error(String(response.status));
  return response.status === 204 ? null : response.json().catch(() => null);
}

async function registerWorker(): Promise<ServiceWorkerRegistration | null> {
  if (!("serviceWorker" in navigator)) return null;
  return navigator.serviceWorker.register("sw.js", { scope: "./" });
}

async function enablePush(): Promise<void> {
  const status = $('[data-part="push-status"]');
  try {
    const registration = await registerWorker();
    if (!registration || !("pushManager" in registration)) {
      status.textContent = "이 브라우저는 알림을 받을 수 없어요. 홈 화면에 추가한 뒤 다시 해 보세요.";
      return;
    }
    const permission = await Notification.requestPermission();
    if (permission !== "granted") {
      status.textContent = "알림 권한이 없어요.";
      return;
    }
    const { key } = await getJson<{ key: string }>(API.pushKey);
    const subscription = await registration.pushManager.subscribe({
      userVisibleOnly: true,
      applicationServerKey: base64UrlToBytes(key),
    });
    const json = subscription.toJSON();
    await postJson(API.pushSubscribe, {
      endpoint: json.endpoint,
      p256dh: json.keys?.p256dh,
      auth: json.keys?.auth,
      device: navigator.userAgent.slice(0, 60),
    });
    status.textContent = "알림을 켰어요.";
  } catch (error) {
    status.textContent = "알림을 켜지 못했어요: " + message(error);
  }
}

function addNotice(notice: Notice): void {
  const item = document.createElement("li");
  item.dataset.seq = String(notice.seq);
  const title = document.createElement("strong");
  title.textContent = notice.title;
  const body = document.createElement("div");
  body.className = "muted";
  body.textContent = notice.body;
  item.append(title, body);
  const link = notice.link;
  if (link) {
    item.addEventListener("click", () => openLink(link));
  }
  $("#notice-list").prepend(item);
}

async function loadNotices(): Promise<void> {
  try {
    const list = await getJson<Notice[]>(API.notices + "?after=" + lastSeq + "&max=50");
    for (const notice of list) {
      addNotice(notice);
      lastSeq = Math.max(lastSeq, notice.seq);
    }
  } catch {
    // 서버가 잠시 안 되면 다음에 다시 읽는다.
  }
}

async function showApproval(id: string): Promise<void> {
  const card = $('[data-card="approval"]');
  try {
    const approval = await getJson<{ fingerprint?: string; summary?: string }>(approvalPath(id));
    card.dataset.approvalId = id;
    card.dataset.fingerprint = approval.fingerprint || "";
    $part(card, "summary").textContent = approval.summary || "";
    $part(card, "fingerprint").textContent = approval.fingerprint || "";
    $part(card, "result").textContent = "";
    // 새 카드는 앞 카드의 보내는 중 표시를 이어받지 않는다(19차 5번).
    delete card.dataset.deciding;
    // 결정 4번: 지문 앞 6자리를 보이기만 한다. 사용자가 치지 않는다.
    (card.querySelector('[data-field="code"]') as HTMLInputElement).value = (approval.fingerprint || "").slice(0, 6);
    setDecideButtons(card, false);
    card.hidden = false;
  } catch {
    card.hidden = true;
  }
}

function $part(card: HTMLElement, name: string): HTMLElement {
  return card.querySelector('[data-part="' + name + '"]') as HTMLElement;
}

/** 카드의 승인 · 거절 단추를 함께 막거나 켠다. */
function setDecideButtons(card: HTMLElement, locked: boolean): void {
  for (const verb of ["approve", "reject"]) {
    const button = card.querySelector<HTMLButtonElement>('[data-action="' + verb + '"]');
    if (button) button.disabled = locked;
  }
}

async function decide(verb: Verb): Promise<void> {
  const card = $('[data-card="approval"]');
  const result = $part(card, "result");
  if (!card.dataset.approvalId || !card.dataset.fingerprint) {
    result.textContent = "승인 내용을 아직 받지 못했어요.";
    return;
  }
  // 보내는 동안 두 단추를 막는다(첫 클릭 안에서 바로). 성공하면 막힌 채 두고, 실패하면 다시 켠다.
  if (card.dataset.deciding) return;
  // (19차 5번) 보낸 승인 id를 기억한다. 답이 왔을 때 카드가 다른 승인으로 바뀌었으면 그 카드의 결과 글 · 단추를 건드리지 않는다.
  const sentId = card.dataset.approvalId;
  card.dataset.deciding = sentId;
  setDecideButtons(card, true);
  try {
    // 화면이 받은 승인 카드의 지문을 그대로 보낸다(서버가 지금 내용의 지문과 비교한다).
    await postJson(decidePath(sentId, verb), {
      fingerprint: card.dataset.fingerprint,
    });
    if (card.dataset.approvalId === sentId) {
      result.textContent = verb === "approve" ? "승인했어요." : "거절했어요.";
    }
  } catch (error) {
    if (card.dataset.approvalId === sentId) {
      result.textContent = "처리하지 못했어요: " + message(error);
      setDecideButtons(card, false);
    }
  } finally {
    if (card.dataset.deciding === sentId) delete card.dataset.deciding;
  }
}

async function loadDailySummary(): Promise<void> {
  try {
    const summary = await getJson<Record<string, string | undefined>>(API.dailySummary);
    for (const part of ["done", "pnl", "review"]) {
      const box = document.querySelector('#daily-summary [data-part="' + part + '"] p');
      const text = summary[part];
      if (box && text) box.textContent = text;
    }
  } catch {
    // 아직 하루 정리가 없다.
  }
}

function openLink(link: string): void {
  const match = /\/approvals\/([^/?#]+)/.exec(link);
  if (match) void showApproval(decodeURIComponent(match[1]));
}

document.addEventListener("click", (event) => {
  const target = event.target as Element | null;
  const action = target?.closest<HTMLElement>("[data-action]");
  if (!action) return;
  switch (action.dataset.action) {
    case "enable-push":
      void enablePush();
      break;
    case "approve":
      void decide("approve");
      break;
    case "reject":
      void decide("reject");
      break;
    case "open-office":
      window.location.href = OFFICE_PATH;
      break;
    default:
      break;
  }
});

if ("serviceWorker" in navigator) {
  navigator.serviceWorker.addEventListener("message", (event: MessageEvent) => {
    const data = (event.data || {}) as { type?: string; link?: string };
    if (data.type === "notice") void loadNotices();
    if (data.type === "open" && data.link) openLink(new URL(data.link).pathname);
  });
}

registerWorker().catch(() => {});
openLink(window.location.pathname + window.location.hash.replace("#", ""));
void loadNotices();
void loadDailySummary();
