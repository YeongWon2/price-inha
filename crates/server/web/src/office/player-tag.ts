// 내 캐릭터 이름표(server 32차 3번): 담당과 같은 이름표(`.tag`, 발 아래 자리 잡기는 `layout.ts` 그대로)이되
// 원본 DeskRPG의 내 이름표처럼 녹색 바탕(#4e745b)에 흰 글자다(docs/research/deskrpg.md 11절). 색은 `office.css`의
// `.tag.me` 규칙에 둔다. 글은 `textContent`로만 넣는다.

/** 내 이름표 바탕색(원본). */
export const MY_TAG_BACKGROUND = "#4e745b";

/** 내 이름표를 가르는 class(`.tag`에 더한다). */
export const MY_TAG_CLASS = "me";

/** 이름표 요소: `<div class="tag">`(내 것이면 `class="tag me"`), 글은 이름, `title`도 이름. */
export function makeNameTag(doc: Document, name: string, mine: boolean): HTMLElement {
  const tag = doc.createElement("div");
  tag.className = mine ? `tag ${MY_TAG_CLASS}` : "tag";
  tag.textContent = name;
  tag.title = name;
  return tag;
}
