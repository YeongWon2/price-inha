//! (2차, 결정 31) 임베딩 줄이기: f32 벡터를 i8 정수로 양자화해 둔다(벡터마다 크기 값 하나).
//!
//! 바이트 모양: 앞 4바이트 = 크기 값(리틀 엔디언 f32, 벡터 성분의 가장 큰 절댓값 / 127), 뒤 = 차원마다 i8 하나
//! (`round(v / 크기)`). 되살리기는 `i8 × 크기`. 코사인 점수는 되살린 벡터로 계산한다. 옛 f32 줄(길이 차원 × 4)도 읽는다.
//! 원본에 없음(원본은 임베딩을 두지 않는다 — 결정 31의 조정자 해석 "요약 + 압축 보관 + 임베딩 줄이기").

use pi_store::{ChunkEmbedding, ChunkId};

/// i8의 가장 큰 크기.
const LEVELS: f32 = 127.0;

/// f32 벡터를 정수 양자화 바이트로 바꾼다(길이 = 차원 + 4). 유한하지 않은 성분은 0으로 본다.
pub fn quantize(vector: &[f32]) -> Vec<u8> {
    let clean: Vec<f32> = vector
        .iter()
        .map(|v| if v.is_finite() { *v } else { 0.0 })
        .collect();
    let peak = clean.iter().fold(0.0_f32, |m, v| m.max(v.abs()));
    let scale = if peak > 0.0 { peak / LEVELS } else { 0.0 };
    let mut bytes = Vec::with_capacity(clean.len() + 4);
    bytes.extend_from_slice(&scale.to_le_bytes());
    bytes.extend(
        levels(&clean, scale)
            .into_iter()
            .map(|q| q.to_le_bytes()[0]),
    );
    bytes
}

/// 크기 값으로 나눠 반올림한 정수들(-127..=127).
fn levels(clean: &[f32], scale: f32) -> Vec<i8> {
    clean
        .iter()
        .map(|v| {
            let level = if scale > 0.0 {
                (v / scale).round().clamp(-LEVELS, LEVELS)
            } else {
                0.0
            };
            // 범위를 -127..=127로 묶었으므로 i8로 그대로 들어간다.
            level as i8
        })
        .collect()
}

/// 정수 양자화 바이트를 f32 벡터로 되살린다. 길이가 4보다 짧으면 `None`.
pub fn dequantize(bytes: &[u8]) -> Option<Vec<f32>> {
    let (head, body) = bytes.split_first_chunk::<4>()?;
    let scale = f32::from_le_bytes(*head);
    Some(
        body.iter()
            .map(|b| f32::from(i8::from_le_bytes([*b])) * scale)
            .collect(),
    )
}

/// 두 벡터의 코사인 유사도. 길이가 다르거나 한쪽이 0 벡터이면 0.
pub fn cosine(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() {
        return 0.0;
    }
    let (mut dot, mut aa, mut bb) = (0.0_f32, 0.0_f32, 0.0_f32);
    for (x, y) in a.iter().zip(b) {
        dot += x * y;
        aa += x * x;
        bb += y * y;
    }
    if aa <= 0.0 || bb <= 0.0 {
        return 0.0;
    }
    dot / (aa.sqrt() * bb.sqrt())
}

/// 저장된 임베딩을 벡터로(정수 양자화면 되살리고, 옛 f32면 그대로). 길이가 맞지 않으면 `None`.
pub(crate) fn vector_of(embedding: &ChunkEmbedding) -> Option<Vec<f32>> {
    let dims = usize::try_from(embedding.dims).ok()?;
    if embedding.quantized {
        return dequantize(&embedding.bytes).filter(|v| v.len() == dims);
    }
    if embedding.bytes.len() != dims.checked_mul(4)? {
        return None;
    }
    Some(
        embedding
            .bytes
            .as_chunks::<4>()
            .0
            .iter()
            .map(|b| f32::from_le_bytes(*b))
            .collect(),
    )
}

impl crate::Knowledge {
    /// 조각에 둔 벡터(옛 f32 줄이면 그대로, 양자화 줄이면 되살린 것). 없으면 `None`.
    pub fn stored_vector(&self, id: ChunkId) -> Result<Option<Vec<f32>>, crate::Error> {
        Ok(self.store.chunk_embedding(id)?.as_ref().and_then(vector_of))
    }

    /// 후보 조각들을 `query`와의 코사인 점수로 줄 세워 `max`개까지 준다(높은 것부터, 같으면 번호 순).
    /// 벡터가 없거나 차원이 다른 조각은 빠진다.
    pub fn rank_by_vector(
        &self,
        query: &[f32],
        candidates: &[ChunkId],
        max: usize,
    ) -> Result<Vec<(ChunkId, f32)>, crate::Error> {
        let mut scored = Vec::new();
        for &id in candidates {
            let Some(vector) = self.stored_vector(id)? else {
                continue;
            };
            if vector.len() == query.len() {
                scored.push((id, cosine(query, &vector)));
            }
        }
        scored.sort_by(|a, b| b.1.total_cmp(&a.1).then(a.0.cmp(&b.0)));
        scored.truncate(max);
        Ok(scored)
    }
}
