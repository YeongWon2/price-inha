// 사무실 채팅의 "메신저에서" 표시(server 41차 3번 · 결정 26 · V14 · V58): 실시간 연결로 받은 `office.message` 가운데
// 메신저에서 온 글(`via: "messenger"`)은 내 글 줄과 "메신저에서" 덧말이 되고, 화면에서 보낸 글(`via` 없음) · 틀린 모양은
// 아니다. 덧말은 채팅 줄의 이름 옆에 보인다.
import { describe, expect, it } from "vitest";
import { VIA_MESSENGER_TEXT, chatLine, fromMessenger } from "../../web/src/office/chat-lines";
import { chatLineElement } from "../../web/src/office/chat-view";

const PAYLOAD = {
  room: "team",
  agent: null,
  text: "@[전략] 지금 어떻게 보세요?",
  sourceMessageId: "messenger-5001",
  via: "messenger",
};

describe("사무실 채팅 · 메신저에서 온 글", () => {
  it("via가 messenger인 팀 방 글은 내 글 줄과 '메신저에서' 덧말이 된다", () => {
    // 검증: V58 V14
    expect(VIA_MESSENGER_TEXT).toBe("메신저에서");
    const via = fromMessenger(PAYLOAD);
    expect(via).not.toBeNull();
    expect(via?.note).toBe(VIA_MESSENGER_TEXT);
    expect(via?.entry).toEqual({ kind: "me", key: "messenger-5001", text: "@[전략] 지금 어떻게 보세요?" });
  });

  it("화면에서 보낸 글(via 없음)과 틀린 모양은 아니다", () => {
    // 검증: V58
    const { via: _drop, ...local } = PAYLOAD;
    void _drop;
    expect(fromMessenger(local)).toBeNull();
    expect(fromMessenger({ ...PAYLOAD, via: "web" })).toBeNull();
    expect(fromMessenger({ ...PAYLOAD, text: 3 })).toBeNull();
    expect(fromMessenger(null)).toBeNull();
    expect(fromMessenger("글")).toBeNull();
  });

  it("그린 줄에는 글과 '메신저에서' 표시가 함께 보인다", () => {
    // 검증: V58 V14
    const via = fromMessenger(PAYLOAD);
    expect(via).not.toBeNull();
    if (!via) return;
    const node = chatLineElement(chatLine(via.entry, {}), via.note);
    expect(node.textContent).toContain(VIA_MESSENGER_TEXT);
    expect(node.textContent).toContain("지금 어떻게 보세요?");
  });
});
