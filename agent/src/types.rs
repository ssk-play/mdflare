use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct FileItem {
    pub name: String,
    pub path: String,
    #[serde(rename = "type")]
    pub file_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub modified: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub children: Option<Vec<FileItem>>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct FilesResponse {
    pub user: String,
    pub files: Vec<FileItem>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct FileContent {
    pub path: String,
    pub content: String,
    pub size: u64,
    pub modified: String,
}

#[derive(Debug, Deserialize)]
pub struct PutFileRequest {
    pub content: String,
}
