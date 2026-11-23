// 다른 기기의 키 입력 화면(`/keys`). 세션 경로라 로그인한 뒤에만 열린다. 키 값은 원격 등록 경로로
// 본문에만 실어 보내고, 화면 · 브라우저 저장소 어디에도 남기지 않으며, 서버 답으로 칸을 다시 채우지 않는다.
import "./keys.css";
import { API } from "../paths";
import { applyKeyFieldRules, submitKeyFields } from "./field";

// 바깥 키 칸: item 이름 → 칸 이름.
const SLOT_FIELDS: Record<string, string> = {
  dart: "api_key",
  ecos: "api_key",
  fred: "api_key",
  backup_github_token: "token",
};
// 공급자 이름 → 화면에 적는 말(모르는 공급자는 이름 그대로).
const PROVIDER_LABELS: Record<string, string> = { naver_news: "네이버 뉴스 키 (뉴스 검색)" };
const REASONS: Record<string, string> = {
  key_rejected: "키가 맞지 않습니다",
  unreachable: "확인할 곳에 닿지 못했습니다",
  invalid_input: "칸을 다시 확인하세요",
  body_too_large: "값이 너무 깁니다",
  save_failed: "저장하지 못했습니다",
};

/** 칸 이름 → 입력 칸 id. */
type Fields = Record<string, string>;

interface Slot {
  name?: unknown;
  registered?: unknown;
}

interface Provider {
  provider: string;
  fields: unknown[];
  registered?: unknown;
}

function byId<T extends HTMLElement = HTMLElement>(id: string): T {
  return document.getElementById(id) as T;
}

function say(node: HTMLElement, text: unknown): void {
  node.textContent = String(text);
}

function getView(path: string): Promise<Record<string, unknown> | null> {
  return fetch(path, { credentials: "same-origin", cache: "no-store" }).then((reply) =>
    reply.ok ? (reply.json() as Promise<Record<string, unknown>>) : null,
  );
}

// 칸별 등록 여부만 보여 준다(값은 오지 않는다).
function loadSlots(): void {
  getView(API.setupSlots)
    .then((view) => {
      if (!view || !Array.isArray(view.slots)) {
        return;
      }
      (view.slots as Slot[]).forEach((slot) => {
        const node = slot && document.getElementById("k-slot-" + String(slot.name) + "-reg");
        if (node) {
          say(node, slot.registered === true ? "(등록됨)" : "(등록 안 됨)");
        }
      });
    })
    .catch(() => {
      /* 다음 등록 뒤 다시 읽는다 */
    });
  loadProviders();
}

// 공급자 칸 하나(칸마다 비밀 입력 칸 · 등록 단추 · 알림 자리)를 만들고 기존 키 등록 길에 잇는다.
function providerBlock(provider: Provider): void {
  const name = String(provider.provider);
  const $block = document.createElement("div");
  $block.className = "item";
  $block.id = "k-provider-" + name;
  const $title = document.createElement("h2");
  say($title, (PROVIDER_LABELS[name] || name) + " ");
  const $reg = document.createElement("span");
  $reg.id = "k-provider-" + name + "-reg";
  $reg.className = "hint";
  $title.appendChild($reg);
  $block.appendChild($title);
  const fields: Fields = {};
  provider.fields.forEach((field) => {
    const id = "k-provider-" + name + "-" + String(field);
    const $label = document.createElement("label");
    $label.htmlFor = id;
    say($label, String(field));
    const $input = document.createElement("input");
    $input.id = id;
    applyKeyFieldRules($input);
    $block.appendChild($label);
    $block.appendChild($input);
    fields[String(field)] = id;
  });
  const $button = document.createElement("button");
  $button.id = "k-provider-" + name + "-button";
  $button.type = "button";
  say($button, "등록");
  const $message = document.createElement("span");
  $message.id = "k-provider-" + name + "-msg";
  $message.className = "msg";
  $message.setAttribute("aria-live", "polite");
  $block.appendChild($button);
  $block.appendChild($message);
  byId("k-providers").appendChild($block);
  wire($button.id, $message.id, name, fields);
}

// 공급자 칸 목록을 읽어 없는 칸은 그리고 등록 여부를 적는다(값은 오지 않는다).
function loadProviders(): void {
  getView(API.setupProviders)
    .then((view) => {
      if (!view || !Array.isArray(view.providers)) {
        return;
      }
      (view.providers as Provider[]).forEach((provider) => {
        if (!provider || typeof provider.provider !== "string" || !Array.isArray(provider.fields)) {
          return;
        }
        if (!document.getElementById("k-provider-" + provider.provider)) {
          providerBlock(provider);
        }
        const node = byId("k-provider-" + provider.provider + "-reg");
        say(node, provider.registered === true ? "(등록됨)" : "(등록 안 됨)");
      });
    })
    .catch(() => {
      /* 다음 등록 뒤 다시 읽는다 */
    });
}

function wire(buttonId: string, messageId: string, item: string, fields: Fields): void {
  const $button = byId<HTMLButtonElement>(buttonId);
  const $message = byId(messageId);
  $button.addEventListener("click", () => {
    const inputs: Record<string, HTMLInputElement> = {};
    Object.keys(fields).forEach((name) => {
      inputs[name] = byId<HTMLInputElement>(fields[name]);
    });
    $button.disabled = true;
    // 받아들여졌을 때만 칸을 비운다(거절 · 실패면 그대로 둔다).
    void submitKeyFields(inputs, (values) =>
      fetch(API.remoteKeys, {
        method: "POST",
        credentials: "same-origin",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ item: item, fields: values }),
      })
        .then((reply) =>
          reply
            .json()
            .catch(() => null)
            .then((data: { registered?: unknown; error?: unknown } | null) => {
              if (reply.ok && data && data.registered) {
                say($message, "등록했습니다");
                loadSlots();
                return true;
              }
              if (reply.status === 401) {
                say($message, "먼저 로그인하세요");
              } else {
                say($message, (data && REASONS[String(data.error)]) || "등록하지 못했습니다");
              }
              return false;
            }),
        )
        .catch(() => {
          say($message, "서버에 닿지 못했습니다");
          return false;
        }),
    ).then(() => {
      $button.disabled = false;
    });
  });
}

wire("k-broker", "k-broker-msg", "broker", { client_id: "k-client-id", client_secret: "k-client-secret" });
wire("k-ai", "k-ai-msg", "ai_api_key", { api_key: "k-api-key" });
Object.keys(SLOT_FIELDS).forEach((item) => {
  const fields: Fields = {};
  fields[SLOT_FIELDS[item]] = "k-slot-" + item;
  wire("k-slot-" + item + "-button", "k-slot-" + item + "-msg", item, fields);
});
loadSlots();
