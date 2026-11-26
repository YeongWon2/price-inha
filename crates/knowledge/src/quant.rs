//! (2차, 결정 31) 임베딩 줄이기: f32 벡터를 i8 정수로 양자화해 둔다(벡터마다 크기 값 하나).
//!
//! 바이트 모양: 앞 4바이트 = 크기 값(리틀 엔디언 f32, 벡터 성분의 가장 큰 절댓값 / 127), 뒤 = 차원마다 i8 하나
//! (`round(v / 크기)`). 되살리기는 `i8 × 크기`. 코사인 점수는 되살린 벡터로 계산한다. 옛 f32 줄(길이 차원 × 4)도 읽는다.
//! 원본에 없음(원본은 임베딩을 두지 않는다 — 결정 31의 조정자 해석 "요약 + 압축 보관 + 임베딩 줄이기").

use pi_store::ChunkId;

/// f32 벡터를 정수 양자화 바이트로 바꾼다(길이 = 차원 + 4). 유한하지 않은 성분은 0으로 본다.
pub fn quantize(vector: &[f32]) -> Vec<u8> {
    let _ = vector;
    todo!()
}

/// 정수 양자화 바이트를 f32 벡터로 되살린다. 길이가 4보다 짧으면 `None`.
pub fn dequantize(bytes: &[u8]) -> Option<Vec<f32>> {
    let _ = bytes;
    todo!()
}

/// 두 벡터의 코사인 유사도. 길이가 다르거나 한쪽이 0 벡터이면 0.
pub fn cosine(a: &[f32], b: &[f32]) -> f32 {
    let _ = (a, b);
    todo!()
}

impl crate::Knowledge {
    /// 조각에 둔 벡터(옛 f32 줄이면 그대로, 양자화 줄이면 되살린 것). 없으면 `None`.
    pub fn stored_vector(&self, id: ChunkId) -> Result<Option<Vec<f32>>, crate::Error> {
        let _ = id;
        todo!()
    }

    /// 후보 조각들을 `query`와의 코사인 점수로 줄 세워 `max`개까지 준다(높은 것부터, 같으면 번호 순).
    /// 벡터가 없거나 차원이 다른 조각은 빠진다.
    pub fn rank_by_vector(
        &self,
        query: &[f32],
        candidates: &[ChunkId],
        max: usize,
    ) -> Result<Vec<(ChunkId, f32)>, crate::Error> {
        let _ = (query, candidates, max);
        todo!()
    }
}
