// 키 화면(`/keys`)의 세션(기능 H 2번): 그 컴퓨터에서 세션 없이 바로 열었으면 설정 화면처럼 세션을 얻는다.
// (뼈대 — red)

/** 서버를 부르는 함수(시험은 가짜를 넣는다). */
export type Fetcher = (input: string, init?: RequestInit) => Promise<Response>;

/** 주소 이름이 그 컴퓨터(루프백)인가. */
export function isLoopbackHost(_host: string): boolean {
  return false;
}

/** 세션이 쓸 수 있게 됐는지 돌려준다. */
export function ensureKeysSession(_fetcher: Fetcher, _host: string): Promise<boolean> {
  return Promise.resolve(false);
}
