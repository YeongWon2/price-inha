// 근거 카드 시험(server 50차 · C23)의 응답 본: 서버가 정렬 · 세기를 끝낸 담당별 의견 · 팀장 요약 · 찬반 수 · 자료.

/** 의견 넷(뉴스 담당 기권, 복기 담당은 보는 기간 없음)과 팀장 요약이 있는 근거 칸. 차례는 서버가 정한 그대로다. */
export function recordedBasis(): Record<string, unknown> {
  return {
    opinions: [
      {
        agent: "scout",
        name: "뉴스 공시 시세 담당",
        score: 0,
        direction: null,
        confidence: null,
        reason: "자료를 받지 못했다",
        horizon_days: null,
        abstained: true,
      },
      {
        agent: "strategy",
        name: "전략 담당",
        score: 80,
        direction: "bull",
        confidence: "high",
        reason: "실적 개선이 이어진다",
        horizon_days: 20,
        abstained: false,
      },
      {
        agent: "risk",
        name: "리스크 담당",
        score: 60,
        direction: "neutral",
        confidence: "mid",
        reason: "변동성이 크다",
        horizon_days: 5,
        abstained: false,
      },
      {
        agent: "review",
        name: "복기 담당",
        score: 40,
        direction: "bear",
        confidence: "low",
        reason: "",
        horizon_days: null,
        abstained: false,
      },
    ],
    leader: {
      decision: "buy",
      summary: "실적이 좋아지고 거래량이 늘었다. 비중은 작게 시작한다.",
      flip_condition: "거래량이 다시 줄면 뒤집힌다",
    },
    votes: { bull: 1, bear: 1, neutral: 1, abstain: 1 },
    sources: [
      { name: "시세", known_at: 1 },
      { name: "주요사항보고서(자기주식취득결정)", known_at: 2 },
    ],
  };
}

/** 칸이 생기기 전(옛) 매매의 근거 칸: 의견 · 팀장 요약 기록이 없다. */
export function oldBasis(): Record<string, unknown> {
  return {
    opinions: [],
    leader: null,
    votes: { bull: 0, bear: 0, neutral: 0, abstain: 0 },
    sources: [{ name: "시세", known_at: 1 }],
  };
}
