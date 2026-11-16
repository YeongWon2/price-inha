//! GitHub contents API 손님(D7 · C13). 토큰은 요청 머리에만 싣고 오류 · 기록에 넣지 않는다(P22).

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use pi_secrets::SecretString;
use pi_store::UnixMillis;
use reqwest::{Client, RequestBuilder, Response};
use serde_json::{Value, json};

use crate::{BackupError, parse_backup_file_name};

const USER_AGENT: &str = "price-inha-backup";
const ACCEPT: &str = "application/vnd.github+json";
const API_VERSION: &str = "2022-11-28";

/// 원격 폴더에 있는 백업 파일 하나.
pub(crate) struct RemoteBackup {
    /// 이름에서 읽은 시각.
    pub(crate) at: UnixMillis,
    /// 저장소 안 경로.
    pub(crate) path: String,
    /// 지울 때 필요한 원격 sha.
    pub(crate) sha: String,
}

/// 한 저장소의 contents API.
pub(crate) struct Remote {
    client: Client,
    contents: String,
    token: SecretString,
}

fn remote_err(status: Option<u16>) -> BackupError {
    BackupError::Remote { status }
}

impl Remote {
    /// 손님을 만든다. 네트워크는 쓰지 않는다.
    pub(crate) fn new(
        api_base: &str,
        repo: &str,
        token: SecretString,
    ) -> Result<Self, BackupError> {
        let client = Client::builder()
            .user_agent(USER_AGENT)
            .build()
            .map_err(|_| remote_err(None))?;
        let contents = format!("{}/repos/{repo}/contents", api_base.trim_end_matches('/'));
        Ok(Self {
            client,
            contents,
            token,
        })
    }

    fn request(&self, builder: RequestBuilder) -> RequestBuilder {
        builder
            .bearer_auth(self.token.expose())
            .header("accept", ACCEPT)
            .header("x-github-api-version", API_VERSION)
    }

    fn url(&self, path: &str) -> String {
        format!("{}/{path}", self.contents)
    }

    async fn send(&self, builder: RequestBuilder) -> Result<Response, BackupError> {
        // 닿지 못한 오류에는 주소가 들어 있을 수 있어 담지 않는다.
        self.request(builder)
            .send()
            .await
            .map_err(|_| remote_err(None))
    }

    /// 새 파일로 올리고 원격이 알려 준 크기를 돌려준다.
    pub(crate) async fn put(&self, path: &str, bytes: &[u8]) -> Result<u64, BackupError> {
        let body = json!({
            "message": format!("price-inha backup {path}"),
            "content": STANDARD.encode(bytes),
        });
        let response = self
            .send(self.client.put(self.url(path)).json(&body))
            .await?;
        let status = response.status();
        if !status.is_success() {
            return Err(remote_err(Some(status.as_u16())));
        }
        let made: Value = response
            .json()
            .await
            .map_err(|_| remote_err(Some(status.as_u16())))?;
        made["content"]["size"]
            .as_u64()
            .ok_or_else(|| remote_err(Some(status.as_u16())))
    }

    /// 폴더 안의 백업 파일들(이름 모양이 맞는 파일만). 폴더가 없으면 빈 목록.
    pub(crate) async fn list(&self, dir: &str) -> Result<Vec<RemoteBackup>, BackupError> {
        let response = self.send(self.client.get(self.url(dir))).await?;
        let status = response.status();
        if status.as_u16() == 404 {
            return Ok(Vec::new());
        }
        if !status.is_success() {
            return Err(remote_err(Some(status.as_u16())));
        }
        let listed: Value = response
            .json()
            .await
            .map_err(|_| remote_err(Some(status.as_u16())))?;
        let Some(entries) = listed.as_array() else {
            return Err(remote_err(Some(status.as_u16())));
        };
        Ok(entries
            .iter()
            .filter_map(|e| backup_entry(dir, e))
            .collect())
    }

    /// 파일 하나를 지운다.
    pub(crate) async fn delete(&self, path: &str, sha: &str) -> Result<(), BackupError> {
        let body = json!({
            "message": format!("price-inha backup retention {path}"),
            "sha": sha,
        });
        let response = self
            .send(self.client.delete(self.url(path)).json(&body))
            .await?;
        let status = response.status();
        if status.is_success() {
            Ok(())
        } else {
            Err(remote_err(Some(status.as_u16())))
        }
    }
}

fn backup_entry(dir: &str, entry: &Value) -> Option<RemoteBackup> {
    if entry["type"].as_str() != Some("file") {
        return None;
    }
    let name = entry["name"].as_str()?;
    let at = parse_backup_file_name(name)?;
    Some(RemoteBackup {
        at,
        path: format!("{dir}/{name}"),
        sha: entry["sha"].as_str()?.to_owned(),
    })
}
