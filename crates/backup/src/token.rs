//! GitHub 토큰 확인(E4). `GET {api_base}/user` 한 번.

use pi_secrets::{SecretString, VerifyFailure};

/// 토큰이 GitHub에서 받아들여지는지 한 번 확인한다(뼈대).
pub async fn verify_token(_api_base: &str, _token: &SecretString) -> Result<(), VerifyFailure> {
    Err(VerifyFailure::Rejected)
}
