//! 다시 색인하는 조건.

use pi_knowledge::{IndexSettings, ReindexPlan, ReindexReason};

use crate::common::Fixture;

fn base() -> IndexSettings {
    IndexSettings {
        provider: "fake".to_string(),
        model: "fake-embed-1".to_string(),
        dims: Some(3),
        chunk_tokens: 400,
        chunk_overlap: 80,
        tokenizer: "trigram".to_string(),
        scope_hash: "scope-a".to_string(),
    }
}

fn plan(f: &Fixture, current: &IndexSettings) -> ReindexPlan {
    f.knowledge
        .plan_reindex(current, true, false)
        .expect("판단")
}

// 검증: D10
#[test]
fn a_new_index_is_built_the_first_time() {
    let f = Fixture::new();
    assert_eq!(
        plan(&f, &base()),
        ReindexPlan::Rebuild(ReindexReason::FirstTime)
    );
    assert_eq!(f.knowledge.stored_settings().expect("읽기"), None);
}

// 검증: D10
#[test]
fn the_same_settings_keep_the_index() {
    let f = Fixture::new();
    f.knowledge.record_settings(&base()).expect("남기기");
    assert_eq!(plan(&f, &base()), ReindexPlan::Keep);
    assert_eq!(f.knowledge.stored_settings().expect("읽기"), Some(base()));
}

// 검증: D10
#[test]
fn each_changed_setting_names_its_reason() {
    let f = Fixture::new();
    f.knowledge.record_settings(&base()).expect("남기기");
    let cases = [
        (
            IndexSettings {
                provider: "other".into(),
                ..base()
            },
            ReindexReason::ProviderChanged,
        ),
        (
            IndexSettings {
                model: "other".into(),
                ..base()
            },
            ReindexReason::ModelChanged,
        ),
        (
            IndexSettings {
                dims: Some(8),
                ..base()
            },
            ReindexReason::DimsChanged,
        ),
        (
            IndexSettings {
                chunk_tokens: 200,
                ..base()
            },
            ReindexReason::ChunkSettingsChanged,
        ),
        (
            IndexSettings {
                chunk_overlap: 40,
                ..base()
            },
            ReindexReason::ChunkSettingsChanged,
        ),
        (
            IndexSettings {
                tokenizer: "unicode61".into(),
                ..base()
            },
            ReindexReason::TokenizerChanged,
        ),
        (
            IndexSettings {
                scope_hash: "scope-b".into(),
                ..base()
            },
            ReindexReason::ScopeChanged,
        ),
    ];
    for (current, reason) in cases {
        assert_eq!(
            plan(&f, &current),
            ReindexPlan::Rebuild(reason),
            "{current:?}"
        );
    }
}

// 검증: D10
#[test]
fn force_rebuilds_even_when_nothing_changed() {
    let f = Fixture::new();
    f.knowledge.record_settings(&base()).expect("남기기");
    let forced = f.knowledge.plan_reindex(&base(), true, true).expect("판단");
    assert_eq!(forced, ReindexPlan::Rebuild(ReindexReason::Forced));
}

// 검증: D10 V23
#[test]
fn a_dead_provider_never_overwrites_a_vector_index() {
    let f = Fixture::new();
    f.knowledge.record_settings(&base()).expect("남기기");
    let changed = IndexSettings {
        model: "other".into(),
        ..base()
    };
    let skipped = f
        .knowledge
        .plan_reindex(&changed, false, false)
        .expect("판단");
    assert_eq!(skipped, ReindexPlan::SkipProviderUnavailable);
    let forced = f
        .knowledge
        .plan_reindex(&changed, false, true)
        .expect("판단");
    assert_eq!(forced, ReindexPlan::SkipProviderUnavailable);
}

// 검증: D10 V23
#[test]
fn a_keyword_only_index_proceeds_without_a_provider() {
    let f = Fixture::new();
    let keyword_only = IndexSettings {
        provider: "none".into(),
        model: String::new(),
        dims: None,
        ..base()
    };
    f.knowledge.record_settings(&keyword_only).expect("남기기");
    let changed = IndexSettings {
        scope_hash: "scope-b".into(),
        ..keyword_only.clone()
    };
    let got = f
        .knowledge
        .plan_reindex(&changed, false, false)
        .expect("판단");
    assert_eq!(got, ReindexPlan::Rebuild(ReindexReason::ScopeChanged));
    // 새 색인도 공급자가 없다고 막지 않는다.
    let fresh = Fixture::new();
    let got = fresh
        .knowledge
        .plan_reindex(&keyword_only, false, false)
        .expect("판단");
    assert_eq!(got, ReindexPlan::Rebuild(ReindexReason::FirstTime));
}

// 검증: D10
#[test]
fn recorded_settings_survive_reopening() {
    let f = Fixture::new();
    f.knowledge.record_settings(&base()).expect("남기기");
    let f = f.reopen();
    assert_eq!(f.knowledge.stored_settings().expect("읽기"), Some(base()));
    assert_eq!(plan(&f, &base()), ReindexPlan::Keep);
}
