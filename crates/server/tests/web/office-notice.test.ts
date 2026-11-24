// 팀 방 전체 공지(server 33차 6번, C19 · F11 · E45 · V71 · V72 · V73): 공지 모드 · 보낼 길 · 공지 띠 · 담당별 확인 상태
// 그리기 · 확인은 한 번만 바뀜 · 지난 공지 채우기 · 보내기 요청.
import { beforeEach, describe, expect, it } from "vitest";
import { API } from "../../web/src/paths";
import {
  NOTICE_BAND_CLASS,
  NOTICE_BUTTON,
  NOTICE_HISTORY_LIMIT,
  NOTICE_MODE_LABEL,
  ackLabel,
  applyAckEvent,
  applyNoticeEvent,
  loadNotices,
  mountNoticeToggle,
  noticeElement,
  noticesFromList,
  postNotice,
  sendRoute,
  type Fetcher,
  type NoticeItem,
} from "../../web/src/office/notice-view";

const AGENTS = ["news", "strategy", "risk", "review", "trader", "leader"];
const NAMES: Record<string, string> = {
  news: "뉴스 공시 시세",
  strategy: "전략",
  risk: "리스크",
  review: "복기",
  trader: "매매",
  leader: "팀장",
};

interface Call {
  url: string;
  init?: RequestInit;
}

function fakeFetch(answer: () => Promise<Response>): { fetcher: Fetcher; calls: Call[] } {
  const calls: Call[] = [];
  const fetcher: Fetcher = (url, init) => {
    calls.push({ url, init });
    return answer();
  };
  return { fetcher, calls };
}

const json = (status: number, body: unknown): Promise<Response> =>
  Promise.resolve(new Response(JSON.stringify(body), { status, headers: { "Content-Type": "application/json" } }));

function waitingNotice(notice: number, text: string, at: number): NoticeItem {
  return { notice, text, at, acks: AGENTS.map((agent) => ({ agent, state: "waiting" as const })) };
}

let host: HTMLElement;
beforeEach(() => {
  document.body.replaceChildren();
  host = document.createElement("div");
  document.body.appendChild(host);
});

describe("공지 모드", () => {
  it("공지 단추를 누르면 공지 모드가 되고 표시가 보인다, 다시 누르면 꺼진다", () => {
    // 검증: V71 C19 F11
    const changes: boolean[] = [];
    const toggle = mountNoticeToggle(host, (on) => changes.push(on));
    expect(toggle.button.textContent).toBe(NOTICE_BUTTON);
    expect(host.contains(toggle.button)).toBe(true);
    expect(toggle.isOn()).toBe(false);
    expect(toggle.marker.hidden).toBe(true);
    toggle.button.click();
    expect(toggle.isOn()).toBe(true);
    expect(toggle.marker.hidden).toBe(false);
    expect(toggle.marker.textContent).toBe(NOTICE_MODE_LABEL);
    expect(toggle.button.getAttribute("aria-pressed")).toBe("true");
    toggle.button.click();
    expect(toggle.isOn()).toBe(false);
    expect(toggle.marker.hidden).toBe(true);
    expect(changes).toEqual([true, false]);
    toggle.set(true);
    expect(toggle.isOn()).toBe(true);
  });

  it("공지 모드여도 멘션이 있으면 멘션 글로 간다", () => {
    // 검증: V71 F11
    expect(sendRoute(true, false)).toBe("notice");
    expect(sendRoute(true, true)).toBe("team");
    expect(sendRoute(false, false)).toBe("team");
    expect(sendRoute(false, true)).toBe("team");
  });
});

describe("공지 띠와 확인 상태", () => {
  it("확인 상태 글", () => {
    // 검증: V72 V73 E45
    expect(ackLabel("answered")).toBe("✓ 답함");
    expect(ackLabel("waiting")).toBe("기다림");
    expect(ackLabel("queued")).toBe("기다림");
    expect(ackLabel("limit")).toBe("상한으로 답 못 함");
    expect(ackLabel("failed")).toBe("실패");
  });

  it("공지는 공지 띠로 보이고 그 아래 담당별 확인이 붙는다", () => {
    // 검증: V71 V72 V73 C19
    const item: NoticeItem = {
      notice: 7,
      text: "이번 주는 반도체 비중을 줄여 주세요.",
      at: 1_000,
      acks: [
        { agent: "news", state: "answered" },
        { agent: "strategy", state: "waiting" },
        { agent: "risk", state: "limit" },
        { agent: "review", state: "failed" },
        { agent: "trader", state: "queued" },
        { agent: "leader", state: "answered" },
      ],
    };
    const band = noticeElement(item, NAMES);
    expect(band.classList.contains(NOTICE_BAND_CLASS)).toBe(true);
    expect(band.dataset.notice).toBe("7");
    expect(band.querySelector(".notice-text")?.textContent).toBe(item.text);
    const acks = Array.from(band.querySelectorAll<HTMLElement>(".notice-acks .ack"));
    expect(acks.map((a) => a.dataset.agent)).toEqual(AGENTS);
    expect(acks.map((a) => a.dataset.state)).toEqual(["answered", "waiting", "limit", "failed", "queued", "answered"]);
    expect(acks[0].textContent).toBe("뉴스 공시 시세 ✓ 답함");
    expect(acks[2].textContent).toBe("리스크 상한으로 답 못 함");
    expect(acks[3].textContent).toBe("복기 실패");
    expect(noticeElement(item, {}).querySelector(".ack")?.textContent).toBe("news ✓ 답함");
  });

  it("공지 글의 꺾쇠는 글로만 보인다(요소가 되지 않는다)", () => {
    // 검증: V71 D1
    const band = noticeElement(waitingNotice(1, "<b>굵게</b>", 1), NAMES);
    expect(band.querySelector("b")).toBeNull();
    expect(band.querySelector(".notice-text")?.textContent).toBe("<b>굵게</b>");
  });

  it("office.notice 이벤트는 여섯 담당이 기다림인 새 공지를 더한다(같은 번호는 한 번)", () => {
    // 검증: V71 C19
    const items = applyNoticeEvent([], { notice: 3, text: "공지", at: 500 }, AGENTS);
    expect(items).toEqual([waitingNotice(3, "공지", 500)]);
    expect(applyNoticeEvent(items, { notice: 3, text: "공지", at: 500 }, AGENTS)).toEqual(items);
    expect(applyNoticeEvent(items, { text: "번호 없음" }, AGENTS)).toEqual(items);
  });

  it("office.notice_ack는 그 담당의 확인을 한 번만 바꾼다", () => {
    // 검증: V72 V73 C19 M10
    let items = [waitingNotice(3, "공지", 500)];
    items = applyAckEvent(items, { notice: 3, agent: "risk", state: "answered" });
    expect(items[0].acks.find((a) => a.agent === "risk")?.state).toBe("answered");
    items = applyAckEvent(items, { notice: 3, agent: "risk", state: "failed" });
    expect(items[0].acks.find((a) => a.agent === "risk")?.state).toBe("answered");
    items = applyAckEvent(items, { notice: 3, agent: "leader", state: "limit" });
    expect(items[0].acks.find((a) => a.agent === "leader")?.state).toBe("limit");
    const same = applyAckEvent(items, { notice: 99, agent: "risk", state: "failed" });
    expect(same).toEqual(items);
    expect(applyAckEvent(items, { notice: 3, agent: "nobody", state: "answered" })).toEqual(items);
  });
});

describe("지난 공지 채우기와 보내기", () => {
  const listed = [
    { notice: 2, text: "둘째", at: 2_000, acks: AGENTS.map((agent) => ({ agent, state: "answered" })) },
    { notice: 1, text: "첫째", at: 1_000, acks: AGENTS.map((agent) => ({ agent, state: "limit" })) },
    { text: "번호 없는 줄" },
  ];

  it("목록 본문은 보낸 순서로 바꾸고 틀린 줄은 버린다", () => {
    // 검증: V71 V72 C19
    const items = noticesFromList(listed);
    expect(items.map((i) => i.notice)).toEqual([1, 2]);
    expect(items[0].acks.every((a) => a.state === "limit")).toBe(true);
    expect(items[1].text).toBe("둘째");
    expect(noticesFromList({ error: "x" })).toEqual([]);
  });

  it("사무실에 들어오면 지난 공지를 읽어 채운다(실패하면 빈 목록)", async () => {
    // 검증: V71 C19
    const { fetcher, calls } = fakeFetch(() => json(200, listed));
    const items = await loadNotices(fetcher);
    expect(items.map((i) => i.text)).toEqual(["첫째", "둘째"]);
    expect(calls.map((c) => c.url)).toEqual([`${API.officeNotices}?limit=${NOTICE_HISTORY_LIMIT}`]);
    expect(await loadNotices(fakeFetch(() => json(401, { error: "unauthorized" })).fetcher)).toEqual([]);
    expect(await loadNotices(fakeFetch(() => Promise.reject(new Error("끊김"))).fetcher, 5)).toEqual([]);
  });

  it("공지로 보내면 공지 경로로 글과 글 번호를 보낸다", async () => {
    // 검증: V71 C19
    const acks = AGENTS.map((agent) => ({ agent, state: "queued" }));
    const { fetcher, calls } = fakeFetch(() => json(200, { notice: 4, acks }));
    const reply = await postNotice(fetcher, "공지 글", "m-9");
    expect(reply).toEqual({ ok: true, notice: 4, acks });
    expect(calls).toHaveLength(1);
    expect(calls[0].url).toBe(API.officeNotice);
    expect(calls[0].init?.method).toBe("POST");
    expect(JSON.parse(String(calls[0].init?.body))).toEqual({ text: "공지 글", sourceMessageId: "m-9" });
  });

  it("보내기가 거절되거나 닿지 못하면 오류 이름을 돌려준다(던지지 않는다)", async () => {
    // 검증: C19 N18
    expect(await postNotice(fakeFetch(() => json(429, { error: "too_fast" })).fetcher, "a", "1")).toEqual({
      ok: false,
      error: "too_fast",
    });
    expect(await postNotice(fakeFetch(() => Promise.reject(new Error("끊김"))).fetcher, "a", "2")).toEqual({
      ok: false,
      error: "network",
    });
  });
});
