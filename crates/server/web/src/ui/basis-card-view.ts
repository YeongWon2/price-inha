// 근거 카드 그리기(server 50차 · C23 · V68 · V69): basis-card.ts가 만든 글을 DOM으로. 글은 모두 글자(textContent)로 넣는다.
// 히스토리 화면의 매매 상세와 사무실의 근거 패널이 같은 카드를 쓴다. 모양은 basis-card.css(토큰만).
// 확신도는 방향 색과 다른 채널(칸 수 막대)로 보인다(docs/research/invest-agent-teams.md F 보조 원칙).
//
// 카드 모양(시험이 고르는 표시):
// - 뿌리 `section.basis-card[data-role="basis-card"]`, 제목 "근거 카드".
// - 기록이 없으면(옛 매매) `[data-role="no-record"]` "기록 없음" 한 줄과 자료 줄만.
// - 찬반 수 `[data-role="votes"]`, 담당 줄 목록 `ol[data-role="opinions"]`의 `li[data-agent=id][data-tone=…]`
//   (기권이면 `data-abstained="true"`): 이름 `[data-part="name"]` · 방향 `[data-part="direction"]` ·
//   확신도 막대 `[data-part="confidence"]`(role="meter", 기권이면 없음) · 이유 `[data-part="reason"]` ·
//   보는 기간 `[data-part="horizon"]`.
// - 팀장 칸 `[data-role="leader"]`: 결정 `[data-part="decision"]` · 요약 `[data-part="summary"]` ·
//   뒤집힐 조건 `[data-part="flip"]`. 자료 이름 `[data-role="sources"]`.
import "./basis-card.css";
import { NONE, NO_RECORD, type BasisCard, type LeaderView, type OpinionRow } from "./basis-card";

/** 확신도 막대의 칸 수. */
const LEVELS = 3;

function make<K extends keyof HTMLElementTagNameMap>(tag: K, className: string, text?: string): HTMLElementTagNameMap[K] {
  const node = document.createElement(tag);
  node.className = className;
  if (text !== undefined) node.textContent = text;
  return node;
}

function part<K extends keyof HTMLElementTagNameMap>(tag: K, name: string, className: string, text?: string): HTMLElementTagNameMap[K] {
  const node = make(tag, className, text);
  node.dataset.part = name;
  return node;
}

/** 확신도 막대: 세 칸 가운데 `level`칸이 찬다. */
function meter(row: OpinionRow): HTMLElement {
  const bar = part("span", "confidence", "basis-meter");
  bar.setAttribute("role", "meter");
  bar.setAttribute("aria-valuemin", "0");
  bar.setAttribute("aria-valuemax", String(LEVELS));
  bar.setAttribute("aria-valuenow", String(row.level));
  bar.setAttribute("aria-label", `확신도 ${row.confidence}`);
  bar.title = `확신도 ${row.confidence}`;
  for (let i = 1; i <= LEVELS; i++) {
    const seg = make("span", "basis-meter-seg");
    seg.dataset.on = String(i <= row.level);
    bar.append(seg);
  }
  return bar;
}

function opinionItem(row: OpinionRow): HTMLElement {
  const item = make("li", "basis-row");
  item.dataset.agent = row.agent;
  item.dataset.tone = row.tone;
  if (row.abstained) item.dataset.abstained = "true";
  const head = make("div", "basis-row-head");
  head.append(part("strong", "name", "basis-name", row.name), part("span", "direction", "basis-chip", row.direction));
  if (!row.abstained) head.append(meter(row));
  head.append(part("span", "horizon", "basis-horizon", row.horizon));
  item.append(head, part("p", "reason", "basis-reason", row.reason));
  return item;
}

function leaderBox(leader: LeaderView | null): HTMLElement {
  const box = make("section", "basis-leader");
  box.dataset.role = "leader";
  box.append(make("h4", "basis-sub", "팀장"));
  if (leader === null) {
    box.append(part("p", "summary", "basis-muted", NONE));
    return box;
  }
  const decision = part("p", "decision", "basis-decision");
  decision.append("결정 ", make("strong", "", leader.decision));
  const flip = part("p", "flip", "basis-flip");
  flip.append(make("span", "basis-flip-label", "이러면 뒤집힌다"), " ", leader.flip);
  box.append(decision, part("p", "summary", "basis-summary", leader.summary), flip);
  return box;
}

/** 근거 카드 한 장. */
export function renderBasisCard(card: BasisCard): HTMLElement {
  const root = make("section", "basis-card");
  root.dataset.role = "basis-card";
  root.append(make("h3", "basis-title", "근거 카드"));
  if (!card.recorded) {
    const none = make("p", "basis-muted", NO_RECORD);
    none.dataset.role = "no-record";
    root.append(none);
  } else {
    const votes = make("p", "basis-votes");
    votes.dataset.role = "votes";
    votes.append(make("span", "basis-sub", "찬반"), " ", card.votes);
    const list = make("ol", "basis-rows");
    list.dataset.role = "opinions";
    for (const row of card.rows) list.append(opinionItem(row));
    root.append(votes, list, leaderBox(card.leader));
  }
  const sources = make("p", "basis-sources");
  sources.dataset.role = "sources";
  sources.append(make("span", "basis-sub", "자료"), " ", card.sources);
  root.append(sources);
  return root;
}
