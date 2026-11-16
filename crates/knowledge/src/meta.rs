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

fn chunk_key(id: ChunkId) -> String {
    format!("chunk.{}", id.0)
}

/// 조각을 올릴 때 줄 범위와 기록을 남긴다.
pub(crate) fn remember_chunk(
    store: &Store,
    id: ChunkId,
    kind: KnowledgeKind,
    record_ref: &str,
    lines: (u32, u32),
) -> Result<(), Error> {
    let value = format!("{}:{}:{}:{record_ref}", lines.0, lines.1, kind_code(kind));
    store.set_knowledge_meta(&chunk_key(id), &value)?;
    Ok(())
}

/// 지운 조각의 줄 범위 메타를 비운다(저장소에 메타를 지우는 길이 없어 빈 값으로 둔다. 빈 값은 "없음"이다).
pub(crate) fn forget_chunk(store: &Store, id: ChunkId) -> Result<(), Error> {
    if store.knowledge_meta(&chunk_key(id))?.is_some() {
        store.set_knowledge_meta(&chunk_key(id), "")?;
    }
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

const SETTINGS_KEYS: [&str; 7] = [
    "settings.provider",
    "settings.model",
    "settings.dims",
    "settings.chunk_tokens",
    "settings.chunk_overlap",
    "settings.tokenizer",
    "settings.scope_hash",
];

pub(crate) fn save_settings(store: &Store, s: &IndexSettings) -> Result<(), Error> {
    let dims = s.dims.map(|d| d.to_string()).unwrap_or_default();
    let values = [
        s.provider.clone(),
        s.model.clone(),
        dims,
        s.chunk_tokens.to_string(),
        s.chunk_overlap.to_string(),
        s.tokenizer.clone(),
        s.scope_hash.clone(),
    ];
    for (key, value) in SETTINGS_KEYS.iter().zip(values.iter()) {
        store.set_knowledge_meta(key, value)?;
    }
    Ok(())
}

pub(crate) fn load_settings(store: &Store) -> Result<Option<IndexSettings>, Error> {
    let mut values: Vec<String> = Vec::with_capacity(SETTINGS_KEYS.len());
    for key in SETTINGS_KEYS {
        match store.knowledge_meta(key)? {
            Some(v) => values.push(v),
            None => return Ok(None),
        }
    }
    let broken = |what: &str| Error::InvalidRecord(format!("남은 색인 설정이 깨졌다: {what}"));
    let dims = if values[2].is_empty() {
        None
    } else {
        Some(values[2].parse().map_err(|_| broken("dims"))?)
    };
    Ok(Some(IndexSettings {
        provider: values[0].clone(),
        model: values[1].clone(),
        dims,
        chunk_tokens: values[3].parse().map_err(|_| broken("chunk_tokens"))?,
        chunk_overlap: values[4].parse().map_err(|_| broken("chunk_overlap"))?,
        tokenizer: values[5].clone(),
        scope_hash: values[6].clone(),
    }))
}
