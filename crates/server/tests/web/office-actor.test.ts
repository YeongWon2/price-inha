// 사무실 캐릭터(server 20차 2번 · 4번): 원본 외형 GLB를 같은 출처 경로로 불러오고, 불러오기에 실패하면 코드 인형으로 그린다.
// 3D 장면 전체는 jsdom에서 돌릴 수 없어 "캐릭터 만들기"를 불러오기 함수를 넘겨받는 모양으로 시험한다.
import { describe, expect, it } from "vitest";
import {
  CROSSFADE_SECONDS,
  LOOK_DIR,
  OFFICE_LOOKS,
  clipName,
  defaultLook,
  lookUrl,
  makeActor,
  type AgentId,
  type LoadedLook,
} from "../../web/src/office/actor";

const tables = import.meta.glob<string>("../round23/deskrpg_assets.txt", { query: "?raw", import: "default", eager: true });

const AGENTS: AgentId[] = ["news", "strategy", "risk", "review", "trader", "leader"];

/** 해시 표(원본 자산)의 외형 id 50개. */
function tableLooks(): string[] {
  const texts = Object.values(tables);
  expect(texts.length, "tests/round23/deskrpg_assets.txt가 있다").toBe(1);
  return texts[0]
    .split("\n")
    .map((line) => line.split("  ")[1] ?? "")
    .filter((rel) => rel.startsWith("characters/office/") && rel.endsWith(".glb"))
    .map((rel) => rel.slice("characters/office/".length, -".glb".length))
    .sort();
}

const loaded: LoadedLook = { clips: ["idle", "walk", "sit"] };

describe("사무실 캐릭터", () => {
  it("외형 목록은 원본 외형 50개이고 주소는 같은 출처의 원본 경로다", () => {
    // 검증: V15 V1
    expect([...OFFICE_LOOKS].sort()).toEqual(tableLooks());
    expect(OFFICE_LOOKS.length).toBe(50);
    expect(LOOK_DIR).toBe("/office/deskrpg/characters/office/");
    const url = lookUrl(OFFICE_LOOKS[0]);
    expect(url).toBe(`/office/deskrpg/characters/office/${OFFICE_LOOKS[0]}.glb`);
    expect(url.includes("://")).toBe(false);
  });

  it("기본 외형은 여섯 담당이 모두 다르고 외형 목록 안에 있다", () => {
    // 검증: V15 V1
    const looks = AGENTS.map((a) => defaultLook(a));
    expect(new Set(looks).size).toBe(6);
    for (const look of looks) expect(OFFICE_LOOKS).toContain(look);
  });

  it("불러오기가 되면 원본 GLB 몸으로 만들고 같은 출처 주소를 한 번만 부른다", async () => {
    // 검증: V15 D1
    const calls: string[] = [];
    const look = OFFICE_LOOKS[3];
    const body = await makeActor(look, async (url) => {
      calls.push(url);
      return loaded;
    });
    expect(calls).toEqual([lookUrl(look)]);
    expect(body.kind).toBe("glb");
    if (body.kind === "glb") {
      expect(body.look).toBe(look);
      expect(body.model).toBe(loaded);
    }
  });

  it("GLB 불러오기가 실패하면 던지지 않고 코드 인형으로 그린다", async () => {
    // 검증: V15 D1
    const calls: string[] = [];
    const body = await makeActor(OFFICE_LOOKS[0], async (url) => {
      calls.push(url);
      throw new Error("불러오기 실패 흉내");
    });
    expect(calls.length).toBe(1);
    expect(body).toEqual({ kind: "doll" });
  });

  it("목록에 없는 외형은 주소를 만들어 부르지 않고 코드 인형으로 그린다", async () => {
    // 검증: V15 D1
    const calls: string[] = [];
    for (const bad of ["no-such-look", "../../secret", "", "eunchae-mpfb"]) {
      const body = await makeActor(bad, async (url) => {
        calls.push(url);
        return loaded;
      });
      expect(body, bad).toEqual({ kind: "doll" });
    }
    expect(calls).toEqual([]);
  });

  it("클립은 걸으면 walk, 걷지 않고 좌석 가까우면 sit, 아니면 idle이고 바꿀 때 0.22초 섞는다", () => {
    // 검증: V15
    expect(clipName(true, false)).toBe("walk");
    expect(clipName(true, true)).toBe("walk");
    expect(clipName(false, true)).toBe("sit");
    expect(clipName(false, false)).toBe("idle");
    expect(CROSSFADE_SECONDS).toBe(0.22);
  });
});
