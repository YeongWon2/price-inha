// (결정 36) 체험판 안내 줄: 설정 상태에 `trial: true`(price-inha demo가 만든 체험 폴더)가 오면 시연에서 가짜로 도는
// 자리(증권 키 · AI 연결 · 공시 · 뉴스 키 · 메신저와 자동 실행) 바로 앞에 한 줄씩 그린다. 서버가 내려주는 값으로만 그린다.

/** 안내 줄 하나: 그 자리(id) 바로 앞에 그릴 글. */
export interface TrialSpot {
  anchor: string;
  text: string;
}

/** 시연에서 가짜로 도는 자리들(설정 화면의 차례). */
export const TRIAL_SPOTS: TrialSpot[] = [
  {
    anchor: "reg-broker",
    text: "체험판에서는 증권사 키를 실제로 확인하지 않고 가짜 시세로 돌립니다 · 실제 시세는 실제판(price-inha run)에서 쓸 수 있습니다",
  },
  {
    anchor: "reg-ai",
    text: "체험판에서는 AI를 가짜로 돌립니다(키를 실제로 확인하지 않습니다) · 실제 AI 판단은 실제판에서 쓸 수 있습니다",
  },
  {
    anchor: "reg-slot-dart",
    text: "체험판에서는 공시 · 뉴스를 가짜로 돌립니다 · 실제 공시 · 뉴스는 실제판에서 쓸 수 있습니다",
  },
  {
    anchor: "setup-extras",
    text: "체험판에서는 텔레그램 메신저와 자동 실행(price-inha service)을 쓰지 않습니다 · 실제판에서 쓸 수 있습니다",
  },
];

/** 안내 줄의 표시. */
const ROLE = "trial-note";

/** 설정 상태가 체험판이면 자리마다 안내 줄을 그린다(이미 있으면 그대로). 새로 그린 줄 수. */
export function placeTrialNotes(doc: Document, status: Readonly<Record<string, unknown>>): number {
  if (status.trial !== true) return 0;
  let drawn = 0;
  for (const spot of TRIAL_SPOTS) {
    const found = doc.getElementById(spot.anchor);
    if (!found) continue;
    // 자리 표시가 묶음 제목 안의 배지면 제목 바로 뒤(그 묶음의 첫 줄)에, 아니면 그 자리 바로 앞에 그린다.
    const heading = found.closest("h3, h4");
    const parent = (heading ?? found).parentNode;
    const next = heading ? heading.nextElementSibling : found;
    const existing = (heading ? heading.nextElementSibling : found.previousElementSibling) as HTMLElement | null;
    if (!parent || existing?.dataset.role === ROLE) continue;
    const note = doc.createElement("p");
    note.className = "trial-note";
    note.dataset.role = ROLE;
    note.textContent = spot.text;
    parent.insertBefore(note, next);
    drawn += 1;
  }
  return drawn;
}
