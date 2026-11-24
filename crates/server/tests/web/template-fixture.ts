// 템플릿 화면 시험(server 27차)의 응답 본보기: 읽기 응답 `GET /api/templates/current`의 모양(Rust `TEMPLATES_VIEW_PATH` 문서).
import type { TemplateEntry, TemplateState, TemplatesReply } from "../../web/src/templates/model";

/** 그 상태의 스윙 템플릿 하나(판 셋: 처음 · 1회 위험 줄이기 · 일일 손실 한도 넓히기). */
export function swingEntry(id: number, state: TemplateState): TemplateEntry {
  const editable = state === "simulating";
  return {
    id,
    style: "swing",
    state,
    revision: 3,
    fields: [
      { key: "risk_per_trade_pct", value: "1", cap: "2", editable },
      { key: "daily_loss_limit_pct", value: "4", cap: "5", editable },
      { key: "max_positions", value: 5, cap: null, editable },
    ],
    revisions: [
      { revision: 1, at: 1_763_340_000_000, changes: [], proposal: null },
      {
        revision: 2,
        at: 1_763_340_060_000,
        changes: [{ key: "risk_per_trade_pct", before: "1.5", after: "1", widened: false }],
        proposal: null,
      },
      {
        revision: 3,
        at: 1_763_340_120_000,
        changes: [{ key: "daily_loss_limit_pct", before: "3", after: "4", widened: true }],
        proposal: 9,
      },
    ],
    promotion: [
      { criterion: "trades", need: "30", have: "12", met: false },
      { criterion: "trading_days", need: "20", have: "21", met: true },
      { criterion: "net_pnl", need: "0", have: "-1500", met: false },
      { criterion: "max_drawdown", need: "7.5", have: "3", met: true },
      { criterion: "out_of_sample", need: "0", have: null, met: false },
    ],
    approval: state === "promotion_pending" ? "AP-7" : null,
  };
}

/** 템플릿 하나를 쓰는 읽기 응답(템플릿이 없으면 빈 목록). */
export function replyWith(entry: TemplateEntry | null): TemplatesReply {
  return entry ? { current: entry.id, templates: [entry] } : { current: null, templates: [] };
}
