// 규칙 화면 · 금지 종목 · 업종표 묶음의 화면 모델(server 35차 3번 · 결정 19번 · P5 · P8 · R7-3 · F5):
// 읽기 응답 확인 · 초안에 줄 더하기 · 지우기(서버와 같은 값 검사) · 저장 몸통 · 업종 없는 종목 경고 · 저장 결과 문구.
import { describe, expect, it } from "vitest";
import {
  READONLY_TEXT,
  SECTOR_MAX_CHARS,
  addBanned,
  addSector,
  draftOf,
  isSymbol,
  readConfig,
  removeBanned,
  removeSector,
  saveBody,
  saveMessage,
  unsectoredLine,
  type ConfigDraft,
  type RulesConfigReply,
} from "../../web/src/rules/config-model";

function reply(more: Partial<RulesConfigReply> = {}): RulesConfigReply {
  return {
    banned: [{ market: "us", symbol: "TSLA" }],
    sectors: [{ symbol: "000660", sector: "반도체" }],
    editable: true,
    unsectored: [],
    ...more,
  };
}

function added(result: ReturnType<typeof addBanned>): ConfigDraft {
  if (!result.ok) throw new Error(`더해야 한다: ${result.problem}`);
  return result.draft;
}

function refused(result: ReturnType<typeof addBanned>): string {
  if (result.ok) throw new Error("더하면 안 된다");
  return result.problem;
}

describe("금지 종목 · 업종표 화면 모델 · 읽기", () => {
  it("읽기 응답의 네 칸을 그대로 읽고 모르는 칸은 버린다", () => {
    // 검증: R7-3 V82 P5 P8
    const body = { ...reply({ unsectored: ["005930"] }), extra: 1 };
    expect(readConfig(body)).toEqual(reply({ unsectored: ["005930"] }));
  });

  it("모양이 맞지 않으면 없음이다", () => {
    // 검증: R7-3 E57
    expect(readConfig(null)).toBeNull();
    expect(readConfig([])).toBeNull();
    expect(readConfig({ banned: [], sectors: [] })).toBeNull();
    expect(readConfig({ ...reply(), banned: "TSLA" })).toBeNull();
    expect(readConfig({ ...reply(), editable: "yes" })).toBeNull();
  });

  it("초안은 읽은 값의 복사본이다(초안을 고쳐도 읽은 값은 그대로)", () => {
    // 검증: R7-3 F5
    const read = reply();
    const draft = draftOf(read);
    expect(draft).toEqual({ banned: read.banned, sectors: read.sectors });
    const next = added(addBanned(draft, "kr", "000270"));
    expect(next.banned.length).toBe(2);
    expect(draft.banned.length).toBe(1);
    expect(read.banned.length).toBe(1);
  });
});

describe("금지 종목 · 업종표 화면 모델 · 줄 더하기 · 지우기", () => {
  it("종목 글자는 영문 · 숫자 · 점 · 줄표 1~32자다(앞뒤 공백은 뗀다)", () => {
    // 검증: R7-3 P5
    expect(isSymbol("005930")).toBe(true);
    expect(isSymbol(" BRK.B ")).toBe(true);
    expect(isSymbol("A".repeat(32))).toBe(true);
    expect(isSymbol("A".repeat(33))).toBe(false);
    expect(isSymbol("")).toBe(false);
    expect(isSymbol("   ")).toBe(false);
    expect(isSymbol("삼성")).toBe(false);
    expect(isSymbol("00 5930")).toBe(false);
    expect(isSymbol("A&B")).toBe(false);
  });

  it("금지 종목은 끝에 더하고 종목의 앞뒤 공백을 뗀다", () => {
    // 검증: R7-3 P5 F5
    const draft = added(addBanned(draftOf(reply()), "kr", "  000270 "));
    expect(draft.banned).toEqual([
      { market: "us", symbol: "TSLA" },
      { market: "kr", symbol: "000270" },
    ]);
  });

  it("모르는 시장 · 종목 글자가 아님 · 같은 (시장, 종목)이면 더하지 않고 까닭을 준다", () => {
    // 검증: R7-3 P5 E4
    const draft = draftOf(reply());
    expect(refused(addBanned(draft, "jp", "7203")).length).toBeGreaterThan(0);
    expect(refused(addBanned(draft, "kr", "삼성전자")).length).toBeGreaterThan(0);
    expect(refused(addBanned(draft, "us", " TSLA ")).length).toBeGreaterThan(0);
    expect(added(addBanned(draft, "kr", "TSLA")).banned.length, "시장이 다르면 다른 줄이다").toBe(2);
  });

  it("업종표 줄은 끝에 더하고 종목 · 업종 이름의 앞뒤 공백을 뗀다", () => {
    // 검증: R7-3 P8 F5
    const draft = added(addSector(draftOf(reply()), " 005930 ", "  전자  "));
    expect(draft.sectors).toEqual([
      { symbol: "000660", sector: "반도체" },
      { symbol: "005930", sector: "전자" },
    ]);
  });

  it("업종 이름은 1자 이상 가장 긴 글자 수까지(코드 포인트로 센다)", () => {
    // 검증: R7-3 P8 E4
    const draft = draftOf(reply({ sectors: [] }));
    expect(SECTOR_MAX_CHARS).toBe(40);
    expect(added(addSector(draft, "005930", "가".repeat(SECTOR_MAX_CHARS))).sectors.length).toBe(1);
    expect(added(addSector(draft, "005930", "😀".repeat(SECTOR_MAX_CHARS))).sectors.length).toBe(1);
    expect(refused(addSector(draft, "005930", "가".repeat(SECTOR_MAX_CHARS + 1))).length).toBeGreaterThan(0);
    expect(refused(addSector(draft, "005930", "   ")).length).toBeGreaterThan(0);
  });

  it("종목 글자가 아니거나 이미 업종이 있는 종목이면 더하지 않는다", () => {
    // 검증: R7-3 P8 E4
    const draft = draftOf(reply());
    expect(refused(addSector(draft, "삼성", "전자")).length).toBeGreaterThan(0);
    expect(refused(addSector(draft, "000660", "전자")).length).toBeGreaterThan(0);
  });

  it("지우기는 그 차례의 줄만 빼고, 차례 밖이면 그대로다", () => {
    // 검증: R7-3 F5
    let draft = added(addBanned(draftOf(reply()), "kr", "000270"));
    draft = added(addSector(draft, "005930", "전자"));
    expect(removeBanned(draft, 0).banned).toEqual([{ market: "kr", symbol: "000270" }]);
    expect(removeSector(draft, 1).sectors).toEqual([{ symbol: "000660", sector: "반도체" }]);
    expect(removeBanned(draft, 5).banned).toEqual(draft.banned);
    expect(removeSector(draft, -1).sectors).toEqual(draft.sectors);
    expect(draft.banned.length, "받은 초안은 그대로다").toBe(2);
  });
});

describe("금지 종목 · 업종표 화면 모델 · 저장 · 경고", () => {
  it("저장 몸통은 두 칸뿐이고 초안의 차례 그대로다", () => {
    // 검증: R7-3 F5 D2
    let draft = added(addSector(draftOf(reply()), "005930", "전자"));
    draft = added(addBanned(draft, "kr", "000270"));
    const body = saveBody(draft);
    expect(Object.keys(body).sort()).toEqual(["banned", "sectors"]);
    expect(body).toEqual({
      banned: [
        { market: "us", symbol: "TSLA" },
        { market: "kr", symbol: "000270" },
      ],
      sectors: [
        { symbol: "000660", sector: "반도체" },
        { symbol: "005930", sector: "전자" },
      ],
    });
  });

  it("업종 없는 종목이 있으면 응답 차례로 이은 경고 줄, 없으면 없음이다", () => {
    // 검증: P8 R7-3 V82
    expect(unsectoredLine(reply({ unsectored: ["005930", "035720"] }))).toBe(
      "업종이 없어 살 수 없는 종목: 005930, 035720",
    );
    expect(unsectoredLine(reply())).toBeNull();
  });

  it("저장 결과: 200은 저장함 · 403은 이 컴퓨터에서만 · 400은 올바르지 않음 · 그 밖은 다시 시도", () => {
    // 검증: R7-3 E57 C3 C16
    const saved = saveMessage(200, reply());
    expect(saved.kind).toBe("saved");
    expect(saved.text).toContain("저장했습니다");
    const remote = saveMessage(403, { error: "loopback_only" });
    expect(remote.kind).toBe("failed");
    expect(remote.text).toContain(READONLY_TEXT);
    const invalid = saveMessage(400, { error: "invalid_input" });
    expect(invalid.kind).toBe("failed");
    expect(invalid.text).toContain("올바르지 않");
    for (const status of [0, 500, 503]) {
      const failed = saveMessage(status, null);
      expect(failed.kind).toBe("failed");
      expect(failed.text).toContain("다시 시도");
    }
  });
});
