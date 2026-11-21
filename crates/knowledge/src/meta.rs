//! 저장소 메타에 남기는 것: 조각의 줄 범위 · 출처(저장소에 조각 번호로 찾는 길이 없다), 색인 설정.

use pi_store::{ChunkId, KnowledgeChunk, KnowledgeKind, KnowledgeSource, Store};

use crate::{Error, IndexSettings};

pub(crate) fn kind_code(kind: KnowledgeKind) -> &'static str {
    match kind {
        KnowledgeKind::Trade => "trade",
        KnowledgeKind::Review => "review",
        KnowledgeKind::Mistake => "mistake",
        KnowledgeKind::MarketNote => "market_note",
    }
}

pub(crate) fn kind_from(code: &str) -> Option<KnowledgeKind> {
    KnowledgeKind::ALL
        .into_iter()
        .find(|k| kind_code(*k) == code)
}

pub(crate) fn source_code(source: KnowledgeSource) -> &'static str {
    match source {
        KnowledgeSource::Mine => "mine",
        KnowledgeSource::Team => "team",
        KnowledgeSource::External => "external",
    }
}

/// 조각이 어느 기록의 몇째 줄에서 몇째 줄까지인지.
pub(crate) struct ChunkOrigin {
    pub(crate) start_line: u32,
    pub(crate) end_line: u32,
    pub(crate) kind: KnowledgeKind,
    pub(crate) record_ref: String,
}

/// 조각 메타 키의 앞붙이. 키는 이 뒤에 조각 번호가 붙는다.
pub(crate) const CHUNK_KEY_PREFIX: &str = "chunk.";

fn chunk_key(id: ChunkId) -> String {
    format!("{CHUNK_KEY_PREFIX}{}", id.0)
}

/// 조각 메타의 값: 줄 범위와 기록.
pub(crate) fn chunk_value(kind: KnowledgeKind, record_ref: &str, lines: (u32, u32)) -> String {
    format!("{}:{}:{}:{record_ref}", lines.0, lines.1, kind_code(kind))
}

/// 조각을 올릴 때 줄 범위와 기록을 남긴다(이미 올라온 조각의 빠진 메타를 채울 때).
pub(crate) fn remember_chunk(
    store: &Store,
    id: ChunkId,
    kind: KnowledgeKind,
    record_ref: &str,
    lines: (u32, u32),
) -> Result<(), Error> {
    store.set_knowledge_meta(&chunk_key(id), &chunk_value(kind, record_ref, lines))?;
    Ok(())
}

/// 남긴 조각 정보. 이 모듈을 거치지 않고 올라온 조각이면 `None`.
pub(crate) fn origin_of(store: &Store, id: ChunkId) -> Result<Option<ChunkOrigin>, Error> {
    let Some(value) = store.knowledge_meta(&chunk_key(id))? else {
        return Ok(None);
    };
    let mut parts = value.splitn(4, ':');
    let (Some(start), Some(end), Some(kind), Some(record_ref)) =
        (parts.next(), parts.next(), parts.next(), parts.next())
    else {
        return Ok(None);
    };
    let (Ok(start_line), Ok(end_line), Some(kind)) = (start.parse(), end.parse(), kind_from(kind))
    else {
        return Ok(None);
    };
    Ok(Some(ChunkOrigin {
        start_line,
        end_line,
        kind,
        record_ref: record_ref.to_string(),
    }))
}

/// 조각의 줄 범위. 남긴 정보가 없으면 글의 줄 수로 어림한다(1줄부터).
pub(crate) fn lines_of(store: &Store, chunk: &KnowledgeChunk) -> Result<(u32, u32), Error> {
    if let Some(origin) = origin_of(store, chunk.id)? {
        return Ok((origin.start_line, origin.end_line));
    }
    let count = u32::try_from(chunk.text.lines().count().max(1)).unwrap_or(u32::MAX);
    Ok((1, count))
}

const KEY_PROVIDER: &str = "settings.provider";
const KEY_MODEL: &str = "settings.model";
const KEY_DIMS: &str = "settings.dims";
const KEY_CHUNK_TOKENS: &str = "settings.chunk_tokens";
const KEY_CHUNK_OVERLAP: &str = "settings.chunk_overlap";
const KEY_TOKENIZER: &str = "settings.tokenizer";
const KEY_SCOPE_HASH: &str = "settings.scope_hash";

/// 설정 칸을 쓰는 순서. **모델 칸이 맨 마지막이다**(지시서 4차 3번 4-3).
///
/// 칸은 하나씩 따로 쓰여서(칸마다 트랜잭션이 다르다) 도중에 끊기면 앞의 칸만 새것인 찢어진 설정이 남는다.
/// 모델 칸을 마지막에 쓰면 찢어진 설정의 모델은 언제나 옛것(또는 없음)이라, `reembed_step`의 4-2
/// ("모델은 같은데 차원이 다르다" 오류)에 걸리지 않고 다음 부름이 이어서 `Done`과 함께 온전히 다시 남긴다.
/// 모델 칸이 먼저 쓰이면 "모델은 새것, 차원은 옛것"이 남아 4-2에 영영 막힌다.
const SETTINGS_WRITE_ORDER: [&str; 7] = [
    KEY_PROVIDER,
    KEY_DIMS,
    KEY_CHUNK_TOKENS,
    KEY_CHUNK_OVERLAP,
    KEY_TOKENIZER,
    KEY_SCOPE_HASH,
    KEY_MODEL,
];

/// 그 칸에 쓸 글자. 차원이 없으면(키워드 전용) 빈 글자다.
fn settings_value(key: &str, s: &IndexSettings) -> String {
    match key {
        KEY_PROVIDER => s.provider.clone(),
        KEY_MODEL => s.model.clone(),
        KEY_DIMS => s.dims.map(|d| d.to_string()).unwrap_or_default(),
        KEY_CHUNK_TOKENS => s.chunk_tokens.to_string(),
        KEY_CHUNK_OVERLAP => s.chunk_overlap.to_string(),
        KEY_TOKENIZER => s.tokenizer.clone(),
        _ => s.scope_hash.clone(),
    }
}

pub(crate) fn save_settings(store: &Store, s: &IndexSettings) -> Result<(), Error> {
    for key in SETTINGS_WRITE_ORDER {
        store.set_knowledge_meta(key, &settings_value(key, s))?;
    }
    Ok(())
}

pub(crate) fn load_settings(store: &Store) -> Result<Option<IndexSettings>, Error> {
    // 칸이 하나라도 없으면 남은 설정이 없는 것이다(처음 남기다 끊긴 것도 여기에 든다).
    let read = |key: &str| store.knowledge_meta(key);
    let (
        Some(provider),
        Some(model),
        Some(dims),
        Some(chunk_tokens),
        Some(chunk_overlap),
        Some(tokenizer),
        Some(scope_hash),
    ) = (
        read(KEY_PROVIDER)?,
        read(KEY_MODEL)?,
        read(KEY_DIMS)?,
        read(KEY_CHUNK_TOKENS)?,
        read(KEY_CHUNK_OVERLAP)?,
        read(KEY_TOKENIZER)?,
        read(KEY_SCOPE_HASH)?,
    )
    else {
        return Ok(None);
    };
    let broken = |what: &str| Error::InvalidRecord(format!("남은 색인 설정이 깨졌다: {what}"));
    let dims = if dims.is_empty() {
        None
    } else {
        Some(dims.parse().map_err(|_| broken("dims"))?)
    };
    Ok(Some(IndexSettings {
        provider,
        model,
        dims,
        chunk_tokens: chunk_tokens.parse().map_err(|_| broken("chunk_tokens"))?,
        chunk_overlap: chunk_overlap.parse().map_err(|_| broken("chunk_overlap"))?,
        tokenizer,
        scope_hash,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_model_cell_is_written_last() {
        assert_eq!(SETTINGS_WRITE_ORDER.last(), Some(&KEY_MODEL));
        let dims = SETTINGS_WRITE_ORDER.iter().position(|k| *k == KEY_DIMS);
        let model = SETTINGS_WRITE_ORDER.iter().position(|k| *k == KEY_MODEL);
        assert!(dims < model);
    }

    #[test]
    fn every_cell_is_written_exactly_once() {
        let mut keys = SETTINGS_WRITE_ORDER.to_vec();
        keys.sort_unstable();
        keys.dedup();
        assert_eq!(keys.len(), 7);
    }

    #[test]
    fn each_cell_gets_its_own_value() {
        let s = IndexSettings {
            provider: "p".to_string(),
            model: "m".to_string(),
            dims: Some(8),
            chunk_tokens: 400,
            chunk_overlap: 80,
            tokenizer: "trigram".to_string(),
            scope_hash: "h".to_string(),
        };
        let written: Vec<String> = SETTINGS_WRITE_ORDER
            .iter()
            .map(|k| settings_value(k, &s))
            .collect();
        assert_eq!(written, ["p", "8", "400", "80", "trigram", "h", "m"]);
        let keyword_only = IndexSettings { dims: None, ..s };
        assert_eq!(settings_value(KEY_DIMS, &keyword_only), "");
    }
}
