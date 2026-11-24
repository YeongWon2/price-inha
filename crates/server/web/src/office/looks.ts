// 담당 외형 목록(server 20차 4번): 원본 DeskRPG 외형 목록(`office-looks` · `office-looks-extended`, 커밋 5d80963)의
// id · 한국어 이름 · 분류 · 한 줄 소개를 데이터로 옮겼다(결정 13). 순서도 원본과 같다.
// 이 파일은 사무실 첫 묶음(가벼운 화면 · 외형 고르기)도 쓰므로 자산 경로 · 3D를 부르지 않는다.

/** 원본 분류(`LOOK_CATEGORIES`, "전체" 제외). */
export const LOOK_CATEGORIES = [
  { id: "classic", label: "클래식 수트" },
  { id: "casual", label: "비즈니스 캐주얼" },
  { id: "creative", label: "크리에이티브" },
  { id: "leadership", label: "리더십" },
] as const;

export type LookCategory = (typeof LOOK_CATEGORIES)[number]["id"];

/** 외형 하나(원본 id · 이름 · 분류 · 소개). */
export interface LookInfo {
  id: string;
  name: string;
  category: LookCategory;
  subtitle: string;
}

export const LOOKS: readonly LookInfo[] = [
  { id: "office-jun", name: "서준", category: "classic", subtitle: "첫 출근의 설렘 · 네이비 수트와 사원증" },
  { id: "office-tae", name: "태오", category: "casual", subtitle: "현장을 아는 베테랑 · 셔츠와 느슨한 타이" },
  { id: "office-seo", name: "서린", category: "classic", subtitle: "차분한 추진력 · 차콜 수트와 포니테일" },
  { id: "office-min", name: "민재", category: "classic", subtitle: "정돈된 취향 · 체크 수트와 둥근 안경" },
  { id: "office-do", name: "도윤", category: "casual", subtitle: "든든한 동료 · 니트 베스트와 셔츠" },
  { id: "office-yun", name: "윤서", category: "leadership", subtitle: "단단한 리더 · 네이비 재킷과 와이드 팬츠" },
  { id: "office-ha", name: "하경", category: "creative", subtitle: "선명한 아이디어 · 머스터드 니트와 와이드 팬츠" },
  { id: "office-jin", name: "진우", category: "creative", subtitle: "책을 만드는 사람 · 카멜 코트와 터틀넥" },
  { id: "office-eun", name: "은채", category: "casual", subtitle: "따뜻한 집중력 · 카디건과 플리츠 롱스커트" },
  { id: "office-hyeon", name: "현진", category: "leadership", subtitle: "경험이 만든 여유 · 실버 헤어와 다크 쓰리피스" },
  { id: "office-nari", name: "나리", category: "creative", subtitle: "광고 기획자 · 리본 블라우스와 무릎 스커트" },
  { id: "office-roan", name: "로안", category: "creative", subtitle: "아트디렉터 · 검은 터틀넥과 체크 코트" },
  { id: "office-soi", name: "소이", category: "creative", subtitle: "브랜드 전략가 · 올림머리와 더블 재킷" },
  { id: "office-yul", name: "율", category: "creative", subtitle: "카피라이터 · 곱슬머리와 니트 베스트" },
  { id: "office-bomi", name: "보미", category: "creative", subtitle: "콘텐츠 프로듀서 · 땋은 머리와 긴 치마" },
  { id: "office-jiho", name: "지호", category: "casual", subtitle: "촬영 감독 · 셔츠와 현장 헤드셋" },
  { id: "office-dami", name: "다미", category: "creative", subtitle: "캠페인 매니저 · 스카프와 짧은 단발" },
  { id: "office-seul", name: "슬아", category: "creative", subtitle: "디자인 리서처 · 긴 머리와 넉넉한 니트" },
  { id: "office-kyu", name: "규민", category: "classic", subtitle: "클라이언트 파트너 · 줄무늬 수트와 타이" },
  { id: "office-ara", name: "아라", category: "creative", subtitle: "행사 디렉터 · 포니테일과 활동적인 베스트" },
  { id: "office-ian", name: "이안", category: "casual", subtitle: "백엔드 개발자 · 안경과 후드 집업" },
  { id: "office-rumi", name: "루미", category: "casual", subtitle: "프런트엔드 개발자 · 땋은 머리와 후드" },
  { id: "office-gonu", name: "건우", category: "casual", subtitle: "QA 엔지니어 · 체크 셔츠와 기록 노트" },
  { id: "office-haena", name: "해나", category: "casual", subtitle: "데이터 과학자 · 연구 가운과 올림머리" },
  { id: "office-woojin", name: "우진", category: "casual", subtitle: "사내 IT 담당 · 곱슬머리와 넓은 바지" },
  { id: "office-jua", name: "주아", category: "creative", subtitle: "UX 디자이너 · 물결 머리와 리본 니트" },
  { id: "office-taemin", name: "태민", category: "casual", subtitle: "스크럼 마스터 · 베스트와 사원증" },
  { id: "office-sera", name: "세라", category: "casual", subtitle: "보안 엔지니어 · 긴 머리와 터틀넥 코트" },
  { id: "office-hosu", name: "호수", category: "casual", subtitle: "로봇 연구원 · 짧은 머리와 실험 가운" },
  { id: "office-yena", name: "예나", category: "creative", subtitle: "커뮤니티 매니저 · 단발과 넉넉한 후드" },
  { id: "office-sungho", name: "성호", category: "leadership", subtitle: "운영 총괄 · 실버 가르마와 더블 수트" },
  { id: "office-hyejin", name: "혜진", category: "leadership", subtitle: "재무 책임자 · 올림머리와 차분한 스카프" },
  { id: "office-jungwon", name: "정원", category: "leadership", subtitle: "인사 책임자 · 은빛 단발과 긴 니트" },
  { id: "office-seok", name: "석현", category: "leadership", subtitle: "사업 대표 · 웨이브 헤어와 체크 수트" },
  { id: "office-mira", name: "미라", category: "leadership", subtitle: "크리에이티브 총괄 · 긴 머리와 더블 재킷" },
  { id: "office-kyung", name: "경수", category: "leadership", subtitle: "법무 책임자 · 정돈된 머리와 쓰리피스" },
  { id: "office-yeon", name: "연화", category: "leadership", subtitle: "이사회 의장 · 실버 올림머리와 코트" },
  { id: "office-dohun", name: "도훈", category: "leadership", subtitle: "창업자 · 곱슬머리와 실용적인 후드" },
  { id: "office-suhye", name: "수혜", category: "leadership", subtitle: "영업 총괄 · 포니테일과 선명한 수트" },
  { id: "office-jaewon", name: "재원", category: "leadership", subtitle: "전략 고문 · 은빛 웨이브와 터틀넥" },
  { id: "office-daeun", name: "다은", category: "creative", subtitle: "문학 편집자 · 긴 머리와 긴 스커트" },
  { id: "office-jiseok", name: "지석", category: "creative", subtitle: "번역 편집자 · 곱슬머리와 체크 베스트" },
  { id: "office-seona", name: "선아", category: "creative", subtitle: "북디자이너 · 땋은 머리와 아틀리에 코트" },
  { id: "office-haram", name: "하람", category: "classic", subtitle: "해외 판권 담당 · 스카프와 더블 수트" },
  { id: "office-chan", name: "찬영", category: "casual", subtitle: "제작 관리 · 셔츠와 공방 사원증" },
  { id: "office-eunsol", name: "은솔", category: "creative", subtitle: "교정 교열자 · 올림머리와 니트 카디건" },
  { id: "office-sejin", name: "세진", category: "classic", subtitle: "협상 전문가 · 가르마와 핀스트라이프" },
  { id: "office-hyo", name: "효주", category: "casual", subtitle: "서점 영업 · 짧은 단발과 편안한 베스트" },
  { id: "office-yumin", name: "유민", category: "classic", subtitle: "파트너십 매니저 · 웨이브와 노타이 수트" },
  { id: "office-garam", name: "가람", category: "creative", subtitle: "작가 매니저 · 포니테일과 넉넉한 코트" },
];

/** 담당별 기본 외형(여섯 모두 다르다). 서버 `reach/looks_http.rs`의 기본값과 같다. */
export const DEFAULT_LOOKS: Readonly<Record<string, string>> = {
  news: "office-seo",
  strategy: "office-jaewon",
  risk: "office-sera",
  review: "office-min",
  trader: "office-sejin",
  leader: "office-yun",
};

/** id로 외형 정보를 찾는다(없으면 undefined). */
export function lookInfo(id: string): LookInfo | undefined {
  return LOOKS.find((look) => look.id === id);
}
