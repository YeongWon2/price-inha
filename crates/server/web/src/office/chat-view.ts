// 채팅 패널 줄 그리기(server 23차 8번, 원본 DeskRPG 화면 — docs/research/deskrpg.md 11절): chat-lines.ts `chatLine`이
// 정한 쪽 · 이름 · 글을 DOM으로. 내 글은 오른쪽 녹색 말풍선(흰 글자), 담당 글 · 논의 줄은 왼쪽에 굵은 이름 + 글,
// 답을 기다리는 담당은 `○○: 생각 중` 한 줄.

import { pendingLabel, type ChatLine } from "./chat-lines";

/** 줄 하나. `note`는 이름 옆 작은 덧말(팀 방의 내 글: "→ 받는 담당"), `extra`는 줄에 더 붙일 class. */
export function chatLineElement(line: ChatLine, note?: string, extra?: string): HTMLElement {
  const div = document.createElement("div");
  div.className = `msg ${line.side === "right" ? "me" : "agent"}${extra ? ` ${extra}` : ""}`;
  div.dataset.key = line.key;
  if (line.pending) {
    div.classList.add("pending");
    div.textContent = pendingLabel(line);
    return div;
  }
  if (line.name !== null || note) {
    const who = document.createElement("span");
    who.className = "who";
    if (line.name !== null) {
      const name = document.createElement("strong");
      name.textContent = line.name;
      who.append(name);
    }
    if (line.tag) {
      const tag = document.createElement("small");
      tag.className = "topic";
      tag.textContent = line.tag === "day" ? "하루 판단" : line.tag;
      // (50차 · F7-7) 종목 표시는 누르거나 Enter로 근거 카드를 연다(office/basis-panel.ts).
      if (line.tag !== "day") {
        tag.dataset.symbol = line.tag;
        tag.tabIndex = 0;
      }
      who.append(" ", tag);
    }
    if (note) {
      const small = document.createElement("small");
      small.className = "to";
      small.textContent = note;
      who.append(small);
    }
    div.append(who);
  }
  div.append(document.createTextNode(line.text));
  return div;
}
