// (결정 36) 체험판 안내 줄(뼈대).

/** 안내 줄 하나: 그 자리(id) 바로 앞에 그릴 글. */
export interface TrialSpot {
  anchor: string;
  text: string;
}

/** 시연에서 가짜로 도는 자리들. */
export const TRIAL_SPOTS: TrialSpot[] = [];

/** 설정 상태가 체험판이면 안내 줄을 그린다. 새로 그린 줄 수. */
export function placeTrialNotes(_doc: Document, _status: Readonly<Record<string, unknown>>): number {
  return 0;
}
