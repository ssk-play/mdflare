use serde::Deserialize;
use crate::types::{FileItem, FilesResponse, FileContent};

pub struct ApiClient {
    client: reqwest::blocking::Client,
    base_url: String,
    username: String,
    token: String,
}

impl ApiClient {
    pub fn new(base_url: &str, username: &str, token: &str) -> Self {
        Self {
            client: reqwest::blocking::Client::new(),
            base_url: base_url.trim_end_matches('/').to_string(),
            username: username.to_string(),
            token: token.to_string(),
        }
    }

    pub fn list_files(&self) -> Result<Vec<FileItem>, reqwest::Error> {
        let url = format!("{}/api/{}/files", self.base_url, self.username);
        let resp: FilesResponse = self.client
            .get(&url)
            .header("Authorization", format!("Bearer {}", self.token))
            .send()?
            .json()?;
        Ok(resp.files)
    }

    pub fn get_file(&self, path: &str) -> Result<FileContent, reqwest::Error> {
        let encoded = urlencoding::encode(path);
        let url = format!("{}/api/{}/file/{}", self.base_url, self.username, encoded);
        self.client
            .get(&url)
            .header("Authorization", format!("Bearer {}", self.token))
            .send()?
            .json()
    }

    pub fn put_file(&self, path: &str, content: &str) -> Result<(), Box<dyn std::error::Error>> {
        self.put_file_with_diff(path, content, None, None)
    }

    pub fn put_file_with_diff(
        &self,
        path: &str,
        content: &str,
        old_hash: Option<&str>,
        diff: Option<&serde_json::Value>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let encoded = urlencoding::encode(path);
        let url = format!("{}/api/{}/file/{}", self.base_url, self.username, encoded);
        let mut body = serde_json::json!({ "content": content });
        if let Some(oh) = old_hash {
            body["oldHash"] = serde_json::json!(oh);
        }
        if let Some(d) = diff {
            body["diff"] = d.clone();
        }
        self.client
            .put(&url)
            .header("Authorization", format!("Bearer {}", self.token))
            .json(&body)
            .send()?;
        Ok(())
    }

    pub fn delete_file(&self, path: &str) -> Result<(), Box<dyn std::error::Error>> {
        let encoded = urlencoding::encode(path);
        let url = format!("{}/api/{}/file/{}", self.base_url, self.username, encoded);
        self.client
            .delete(&url)
            .header("Authorization", format!("Bearer {}", self.token))
            .send()?;
        Ok(())
    }

    pub fn put_heartbeat(&self) {
        let url = format!("{}/api/{}/agent-status", self.base_url, self.username);
        self.client
            .put(&url)
            .header("Authorization", format!("Bearer {}", self.token))
            .send()
            .ok();
    }

    pub fn get_sync_config(&self) -> Result<RtdbConfig, Box<dyn std::error::Error>> {
        let url = format!("{}/api/{}/sync-config", self.base_url, self.username);
        let resp: RtdbConfig = self.client
            .get(&url)
            .header("Authorization", format!("Bearer {}", self.token))
            .send()?
            .json()?;
        Ok(resp)
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RtdbConfig {
    pub rtdb_url: String,
    pub rtdb_auth: String,
    pub user_id: String,
}
