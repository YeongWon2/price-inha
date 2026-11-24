// 관리 화면 틀(server 26차 4번 · F12 · R18-3): 계좌 · 템플릿 · 규칙 · 히스토리 화면의 본문 자리에
// "준비 중" 빈 상태를 그리고 세션을 얻는다(내용은 뒤 차수). 부르는 서버 경로는 세션뿐이다.
// 이 묶음은 사무실 3D를 부르지 않는다(가볍게 — N21).
import "./admin.css";
import { API } from "../paths";
import { emptyState, errorLine } from "./parts";

/** 빈 상태 글. */
const PREPARING = "준비 중 — 이 화면의 내용은 다음 단계에서 채워집니다.";

/** 세션을 얻지 못했을 때의 글. */
const SESSION_FAILED = "서버와 연결하지 못했습니다. 잠시 뒤 새로 고침해 주세요.";

/**
 * 본문 자리(`main`)에 "준비 중" 빈 상태를 붙이고 세션을 얻는다. 자리가 없으면 아무것도 하지 않는다.
 * 세션을 얻지 못하면 오류 줄을 하나 더한다.
 */
export async function mountAdminFrame(main: HTMLElement | null): Promise<void> {
  if (!main) return;
  main.append(emptyState(PREPARING));
  try {
    const reply = await fetch(API.session, { method: "POST", credentials: "same-origin", cache: "no-store" });
    if (!reply.ok) throw new Error(`session ${reply.status}`);
  } catch {
    main.append(errorLine(SESSION_FAILED));
  }
}
