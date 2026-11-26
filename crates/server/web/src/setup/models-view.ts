// 설정 화면의 담당 모델 고르기(server 57차 · R18-4 · C32 · V78). 뼈대(red).

/** "직접 입력" 고르기의 값. */
export const MODEL_CUSTOM = "__custom__";

/** 비용 안내. */
export const MODELS_COST_NOTE = "";

/** 언제 쓰이나 안내. */
export const MODELS_APPLY_NOTE = "";

/** 다른 기기 안내. */
export const MODELS_REMOTE_NOTE = "";

/** 저장 안내. */
export const MODELS_SAVED = "";

/** 바꾸기 실패 까닭 → 화면 글. */
export const MODELS_ERRORS: Readonly<Record<string, string>> = {};

/** 담당 한 줄. */
export interface ModelRoleRow {
  agent: string;
  name: string;
  model: string;
  default: string;
}

/** 고를 수 있는 모델 하나. */
export interface ModelChoice {
  id: string;
  label: string;
  note: string;
}

/** `GET /api/setup/models`의 답. */
export interface ModelsReply {
  roles: ModelRoleRow[];
  choices: ModelChoice[];
  editable: boolean;
}
