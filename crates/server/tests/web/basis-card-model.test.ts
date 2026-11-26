// 근거 카드 순수 모델(server 50차 · C23 · E53 · R6-4 · R6-5 · V68 · V69): 서버가 준 의견 · 팀장 요약 · 찬반 수 ·
// 자료를 화면 글로 바꾼다. 차례는 서버가 정한 그대로이고, 기권은 0점이 아니라 "기권", 옛 매매는 "기록 없음".
import { describe, expect, it } from "vitest";
import { ABSTAIN, NONE, NO_RECORD, readBasisCard, voteText } from "../../web/src/ui/basis-card";
import { oldBasis, recordedBasis } from "./basis-card-fixture";

describe("근거 카드 모델", () => {
  it("담당 줄은 서버가 준 차례 그대로이고 방향 · 확신도 · 이유 · 보는 기간이 글이 된다", () => {
    // 검증: V68 R6-4 C23
    const card = readBasisCard(recordedBasis());
    expect(card.recorded).toBe(true);
    expect(card.rows.map((row) => row.agent)).toEqual(["scout", "strategy", "risk", "review"]);
    const strategy = card.rows[1];
    expect(strategy).toMatchObject({
      name: "전략 담당",
      abstained: false,
      tone: "bull",
      direction: "강세",
      confidence: "높음",
      level: 3,
      reason: "실적 개선이 이어진다",
      horizon: "보는 기간 20거래일",
    });
    expect(card.rows[2]).toMatchObject({ tone: "neutral", direction: "중립", confidence: "보통", level: 2 });
    expect(card.rows[2].horizon).toBe("보는 기간 5거래일");
    expect(card.rows[3]).toMatchObject({ tone: "bear", direction: "약세", confidence: "낮음", level: 1 });
  });

  it("의견을 못 낸 담당은 0점이 아니라 기권이고 확신도 막대가 비며 빈 칸은 '없음'이다", () => {
    // 검증: V68 E53
    const card = readBasisCard(recordedBasis());
    const scout = card.rows[0];
    expect(scout).toMatchObject({ abstained: true, tone: "abstain", direction: ABSTAIN, confidence: NONE, level: 0 });
    expect(scout.direction).toBe("기권");
    expect(scout.reason).toBe("자료를 받지 못했다");
    expect(scout.horizon).toBe("보는 기간 없음");
    // 비어 있는 이유는 추정해 채우지 않고 "없음"으로 보인다.
    expect(card.rows[3].reason).toBe(NONE);
    expect(card.rows[3].horizon).toBe("보는 기간 없음");
  });

  it("찬반 수는 서버가 센 넷을 그대로 '강세 · 약세 · 중립 · 기권'으로 보인다", () => {
    // 검증: V68 R4-4
    expect(readBasisCard(recordedBasis()).votes).toBe("강세 1 · 약세 1 · 중립 1 · 기권 1");
    expect(voteText({ bull: 4, bear: 0, neutral: 1, abstain: 1 })).toBe("강세 4 · 약세 0 · 중립 1 · 기권 1");
  });

  it("팀장 결정 · 요약 · 뒤집힐 조건과 자료 이름이 글이 된다", () => {
    // 검증: V69 R6-5
    const card = readBasisCard(recordedBasis());
    expect(card.leader).toEqual({
      decision: "사기",
      summary: "실적이 좋아지고 거래량이 늘었다. 비중은 작게 시작한다.",
      flip: "거래량이 다시 줄면 뒤집힌다",
    });
    expect(card.sources).toBe("시세 · 주요사항보고서(자기주식취득결정)");
    const sell = { ...recordedBasis(), leader: { decision: "sell", summary: "판다", flip_condition: "오르면" } };
    expect(readBasisCard(sell).leader?.decision).toBe("팔기");
    const watch = { ...recordedBasis(), leader: { decision: "watch", summary: "본다", flip_condition: "" } };
    expect(readBasisCard(watch).leader).toEqual({ decision: "지켜보기", summary: "본다", flip: NONE });
  });

  it("칸이 생기기 전 매매는 '기록 없음'이고 자료 이름은 그대로 보인다", () => {
    // 검증: V69 E53
    const card = readBasisCard(oldBasis());
    expect(card.recorded).toBe(false);
    expect(card.rows).toEqual([]);
    expect(card.leader).toBeNull();
    expect(card.sources).toBe("시세");
    expect(NO_RECORD).toBe("기록 없음");
  });

  it("모양이 틀린 몸통은 없는 칸으로 본다(던지지 않는다)", () => {
    // 검증: E53
    for (const wire of [null, undefined, 3, "x", {}, { opinions: "x", leader: 1, votes: null, sources: [1] }]) {
      const card = readBasisCard(wire);
      expect(card.recorded).toBe(false);
      expect(card.rows).toEqual([]);
      expect(card.leader).toBeNull();
      expect(card.sources).toBe(NONE);
      expect(card.votes).toBe("강세 0 · 약세 0 · 중립 0 · 기권 0");
    }
    // 줄 하나가 틀리면 그 줄만 뺀다.
    const mixed = { ...recordedBasis(), opinions: [{ agent: 1 }, (recordedBasis().opinions as unknown[])[1]] };
    expect(readBasisCard(mixed).rows.map((row) => row.agent)).toEqual(["strategy"]);
  });
});
