// 담당 캐릭터 만들기(server 20차 2번 · 4번): 원본 DeskRPG 외형 GLB를 불러와 그리고, 불러오기에 실패하면 코드 인형으로 바꾼다.
// 장면(WebGL) 없이 시험할 수 있게 "불러오기 함수"를 넘겨받는 모양이다. 뼈대만 있다(몸통은 N2가 쓴다).

/** 담당 id(서버 `pi_office::Agent::id`와 같다). */
export type AgentId = "news" | "strategy" | "risk" | "review" | "trader" | "leader";

/** 외형 GLB가 놓인 같은 출처 경로(앞부분). 바깥 주소는 없다. */
export const LOOK_DIR = "/office/deskrpg/characters/office/";

/** 원본 외형 id 50개(`characters/office/<id>.glb`의 `<id>`, 원본 `office-looks`의 id). */
export const OFFICE_LOOKS: readonly string[] = [];

/** 클립을 바꿀 때 섞는 시간(초, 원본 0.22). */
export const CROSSFADE_SECONDS = 0;

/** 불러온 외형: 장면 쪽이 쓰는 값. 시험은 클립 이름만 본다. */
export interface LoadedLook {
  clips: readonly string[];
}

/** 불러오기 함수: 같은 출처 주소를 받아 외형을 돌려주거나 실패하면 거부한다. 장면은 GLTFLoader로 이것을 만든다. */
export type LookLoader = (url: string) => Promise<LoadedLook>;

/** 만들어진 몸: 원본 GLB 또는 코드 인형(불러오기 실패 · 모르는 외형). */
export type ActorBody = { kind: "glb"; look: string; model: LoadedLook } | { kind: "doll" };

/** 외형 id → 같은 출처 GLB 주소(`LOOK_DIR` + id + `.glb`). */
export function lookUrl(look: string): string {
  throw new Error("미구현: lookUrl");
}

/** 담당별 기본 외형(서로 다르다, `OFFICE_LOOKS` 안). 서버가 주는 값이 오기 전의 처음 값이다. */
export function defaultLook(agent: AgentId): string {
  throw new Error("미구현: defaultLook");
}

/** 원본 동작: 걸으면 walk, 걷지 않고 좌석 가까우면 sit, 아니면 idle(앉음은 상태가 아니다). */
export function clipName(walking: boolean, seated: boolean): "idle" | "walk" | "sit" {
  throw new Error("미구현: clipName");
}

/** 외형을 만든다. 모르는 외형 id면 불러오지 않고 인형, 불러오기가 거부되면 인형(던지지 않는다). */
export async function makeActor(look: string, load: LookLoader): Promise<ActorBody> {
  throw new Error("미구현: makeActor");
}
