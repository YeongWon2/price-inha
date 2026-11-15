//! 자료 요청과 받은 자료(Fact)의 모양.

use std::fmt;

use chrono::{DateTime, NaiveDate, Utc};
use pi_secrets::KeySlot;
use rust_decimal::Decimal;

use crate::DataError;

/// 자료 출처. 1차는 DART · ECOS · FRED뿐이다(결정 2026-11-16 17번).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SourceName {
    /// 금융감독원 DART 오픈API(국내 공시 · 재무).
    Dart,
    /// 한국은행 ECOS(국내 금리 · 환율).
    Ecos,
    /// 미국 FRED(미국 금리).
    Fred,
}

impl SourceName {
    /// 모든 출처.
    pub const ALL: [SourceName; 3] = [SourceName::Dart, SourceName::Ecos, SourceName::Fred];

    /// 이 출처의 키를 담는 `pi-secrets` 칸.
    pub fn slot(self) -> KeySlot {
        match self {
            SourceName::Dart => KeySlot::Dart,
            SourceName::Ecos => KeySlot::Ecos,
            SourceName::Fred => KeySlot::Fred,
        }
    }

    /// 그 칸이 자료 출처 칸이면 그 출처. 아니면 `None`.
    pub fn from_slot(slot: KeySlot) -> Option<SourceName> {
        SourceName::ALL
            .into_iter()
            .find(|source| source.slot() == slot)
    }

    /// 보이는 이름: `DART` · `ECOS` · `FRED`.
    pub fn as_str(self) -> &'static str {
        match self {
            SourceName::Dart => "DART",
            SourceName::Ecos => "ECOS",
            SourceName::Fred => "FRED",
        }
    }
}

impl fmt::Display for SourceName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// 자료 종류(C11).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FactKind {
    /// 공시 목록(DART).
    Disclosure,
    /// 재무제표 주요 계정(DART).
    Financials,
    /// 국내 금리(ECOS).
    KoreaRate,
    /// 환율(ECOS).
    KoreaFx,
    /// 미국 금리(FRED).
    UsRate,
}

impl FactKind {
    /// 모든 종류.
    pub const ALL: [FactKind; 5] = [
        FactKind::Disclosure,
        FactKind::Financials,
        FactKind::KoreaRate,
        FactKind::KoreaFx,
        FactKind::UsRate,
    ];

    /// 이 종류를 주는 출처.
    pub fn source(self) -> SourceName {
        match self {
            FactKind::Disclosure | FactKind::Financials => SourceName::Dart,
            FactKind::KoreaRate | FactKind::KoreaFx => SourceName::Ecos,
            FactKind::UsRate => SourceName::Fred,
        }
    }
}

/// DART 공시대상회사 고유번호(8자리 숫자). 종목코드(6자리)와 다르다.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CorpCode(String);

impl CorpCode {
    /// 8자리 ASCII 숫자만 받는다. 아니면 [`DataError::InvalidRequest`]이고 받은 글자를 담지 않는다.
    pub fn parse(text: &str) -> Result<Self, DataError> {
        if text.len() == 8 && text.bytes().all(|b| b.is_ascii_digit()) {
            Ok(Self(text.to_owned()))
        } else {
            Err(DataError::InvalidRequest {
                detail: "DART 고유번호는 8자리 숫자여야 합니다".to_owned(),
            })
        }
    }

    /// 고유번호 글자.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// DART 정기보고서 코드(`reprt_code`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ReportCode {
    /// 1분기보고서 `11013`.
    FirstQuarter,
    /// 반기보고서 `11012`.
    HalfYear,
    /// 3분기보고서 `11014`.
    ThirdQuarter,
    /// 사업보고서 `11011`.
    Annual,
}

impl ReportCode {
    /// DART에 보내는 글자.
    pub fn code(self) -> &'static str {
        match self {
            ReportCode::FirstQuarter => "11013",
            ReportCode::HalfYear => "11012",
            ReportCode::ThirdQuarter => "11014",
            ReportCode::Annual => "11011",
        }
    }
}

/// 재무제표 구분(`fs_div`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FsDiv {
    /// 연결재무제표 `CFS`.
    Consolidated,
    /// 재무제표(개별) `OFS`.
    Separate,
}

impl FsDiv {
    /// DART 응답의 글자.
    pub fn code(self) -> &'static str {
        match self {
            FsDiv::Consolidated => "CFS",
            FsDiv::Separate => "OFS",
        }
    }
}

/// ECOS 주기 코드.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EcosCycle {
    /// 연 `A`.
    Annual,
    /// 분기 `Q`.
    Quarter,
    /// 월 `M`.
    Month,
    /// 일 `D`.
    Day,
}

impl EcosCycle {
    /// ECOS 주소에 넣는 글자.
    pub fn code(self) -> &'static str {
        match self {
            EcosCycle::Annual => "A",
            EcosCycle::Quarter => "Q",
            EcosCycle::Month => "M",
            EcosCycle::Day => "D",
        }
    }
}

/// ECOS 통계 하나(통계표 · 항목 · 주기 · 기간). 기간 글자는 주기에 맞춘다(월이면 `YYYYMM`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EcosSeries {
    /// 통계표 코드(예: 기준금리 표).
    pub stat_code: String,
    /// 항목 코드1.
    pub item_code: String,
    /// 주기.
    pub cycle: EcosCycle,
    /// 시작 시점 글자.
    pub start: String,
    /// 끝 시점 글자.
    pub end: String,
}

/// 무엇을 가져오는가(C11 입력: 종목 또는 지표, 자료 종류).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FactRequest {
    /// DART 공시 목록. 접수일 `from..=to`.
    Disclosures {
        /// 회사.
        corp: CorpCode,
        /// 시작 접수일.
        from: NaiveDate,
        /// 끝 접수일.
        to: NaiveDate,
    },
    /// DART 단일회사 주요계정.
    Financials {
        /// 회사.
        corp: CorpCode,
        /// 사업연도.
        year: u16,
        /// 보고서.
        report: ReportCode,
        /// 연결 · 개별 중 무엇만 돌려줄지.
        fs: FsDiv,
    },
    /// ECOS 금리.
    KoreaRate(EcosSeries),
    /// ECOS 환율.
    KoreaFx(EcosSeries),
    /// FRED 계열 관측값. 관측일 `from..=to`.
    UsRate {
        /// FRED 계열 id.
        series_id: String,
        /// 시작 관측일.
        from: NaiveDate,
        /// 끝 관측일.
        to: NaiveDate,
    },
}

impl FactRequest {
    /// 자료 종류.
    pub fn kind(&self) -> FactKind {
        match self {
            FactRequest::Disclosures { .. } => FactKind::Disclosure,
            FactRequest::Financials { .. } => FactKind::Financials,
            FactRequest::KoreaRate(_) => FactKind::KoreaRate,
            FactRequest::KoreaFx(_) => FactKind::KoreaFx,
            FactRequest::UsRate { .. } => FactKind::UsRate,
        }
    }
}

/// 자료 값.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FactValue {
    /// 수(금액 · 비율). 부동소수점을 쓰지 않는다.
    Number(Decimal),
    /// 글(공시 보고서명 등).
    Text(String),
}

/// 자료를 어디서 받았는가. 주소에는 키가 없다(가려져 있다).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Source {
    /// 출처 이름.
    pub name: SourceName,
    /// 요청 주소. 키 자리는 가려져 있다.
    pub url: String,
}

/// 받은 자료 하나(C11 성공: 자료와 알게 된 시각).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fact {
    /// 종류.
    pub kind: FactKind,
    /// 무엇에 대한 자료인가. DART는 고유번호, ECOS는 `통계표/항목`, FRED는 계열 id.
    pub subject: String,
    /// 이름. DART 공시는 회사명, 재무는 계정명, ECOS는 항목명, FRED는 계열 id.
    pub label: String,
    /// 자료가 가리키는 시점. 재무는 당기일자, ECOS는 시점, FRED는 관측일. 공시는 없다.
    pub period: Option<String>,
    /// 값. 공시는 보고서명(글), 나머지는 수.
    pub value: FactValue,
    /// 단위(통화 · `연%` 등). 출처가 주지 않으면 없다.
    pub unit: Option<String>,
    /// 출처 안의 참조 번호(DART 접수번호).
    pub reference: Option<String>,
    /// 출처와 주소.
    pub source: Source,
    /// 공표일. DART 공시는 접수일자, FRED는 그 값의 real-time 시작일. 출처가 주지 않으면 없다.
    pub published_on: Option<NaiveDate>,
    /// 알게 된 시각: 응답을 받은 시각(주입한 시계, P10).
    pub known_at: DateTime<Utc>,
}

impl Fact {
    /// 이 판단 시각에 이미 알 수 있던 자료인가: `known_at <= decision_at` (V36 · P10).
    pub fn known_by(&self, decision_at: DateTime<Utc>) -> bool {
        self.known_at <= decision_at
    }
}

/// 가져온 결과.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Fetched {
    /// 받은 자료. 비어 있지 않다.
    Facts(Vec<Fact>),
    /// 출처에 그 자료가 없다(E32: 판단을 지켜보기로 두는 근거). 오류가 아니다.
    Missing(FactKind),
}
