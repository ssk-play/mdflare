use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;
use notify_debouncer_mini::{new_debouncer, DebouncedEventKind};
use notify::RecursiveMode;
use serde::Deserialize;

use crate::config::Config;
use crate::cloud::ApiClient;
use crate::local_fs::{scan_local_md_files, flatten_file_paths};
use crate::types::FileItem;

// ============================================================================
// RTDB types and diff helpers
// ============================================================================

#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct RtdbFileEntry {
    pub path: String,
    pub action: String,
    #[allow(dead_code)]
    pub hash: Option<String>,
    pub old_hash: Option<String>,
    pub diff: Option<Vec<serde_json::Value>>,
    pub old_path: Option<String>,
    #[allow(dead_code)]
    pub modified: Option<u64>,
    #[allow(dead_code)]
    pub size: Option<u64>,
}

/// Apply a line-based diff to content.
/// diff ops: {"eq": N}, {"del": N}, {"ins": ["line1", ...]}
pub fn apply_line_diff(old_content: &str, diff: &[serde_json::Value]) -> Option<String> {
    let old_lines: Vec<&str> = old_content.split('\n').collect();
    let mut result = Vec::new();
    let mut pos = 0;

    for op in diff {
        if let Some(eq) = op.get("eq").and_then(|v| v.as_u64()) {
            let eq = eq as usize;
            if pos + eq > old_lines.len() {
                return None; // diff doesn't match
            }
            result.extend_from_slice(&old_lines[pos..pos + eq]);
            pos += eq;
        } else if let Some(del) = op.get("del").and_then(|v| v.as_u64()) {
            let del = del as usize;
            if pos + del > old_lines.len() {
                return None;
            }
            pos += del;
        } else if let Some(ins) = op.get("ins").and_then(|v| v.as_array()) {
            for line in ins {
                if let Some(s) = line.as_str() {
                    result.push(s);
                } else {
                    return None;
                }
            }
        } else {
            return None; // unknown op
        }
    }
    // remaining lines (if any eq ops missed)
    result.extend_from_slice(&old_lines[pos..]);
    Some(result.join("\n"))
}

/// Generate a line-based diff using the `similar` crate.
pub fn generate_line_diff(old_content: &str, new_content: &str) -> serde_json::Value {
    use similar::{ChangeTag, TextDiff};

    let text_diff = TextDiff::from_lines(old_content, new_content);
    let mut ops: Vec<serde_json::Value> = Vec::new();
    let mut eq_count = 0usize;
    let mut del_count = 0usize;
    let mut ins_lines: Vec<String> = Vec::new();

    let flush_del = |ops: &mut Vec<serde_json::Value>, del: &mut usize| {
        if *del > 0 {
            ops.push(serde_json::json!({"del": *del}));
            *del = 0;
        }
    };
    let flush_ins = |ops: &mut Vec<serde_json::Value>, ins: &mut Vec<String>| {
        if !ins.is_empty() {
            ops.push(serde_json::json!({"ins": ins.clone()}));
            ins.clear();
        }
    };
    let flush_eq = |ops: &mut Vec<serde_json::Value>, eq: &mut usize| {
        if *eq > 0 {
            ops.push(serde_json::json!({"eq": *eq}));
            *eq = 0;
        }
    };

    for change in text_diff.iter_all_changes() {
        match change.tag() {
            ChangeTag::Equal => {
                flush_del(&mut ops, &mut del_count);
                flush_ins(&mut ops, &mut ins_lines);
                eq_count += 1;
            }
            ChangeTag::Delete => {
                flush_eq(&mut ops, &mut eq_count);
                flush_ins(&mut ops, &mut ins_lines);
                del_count += 1;
            }
            ChangeTag::Insert => {
                flush_eq(&mut ops, &mut eq_count);
                // strip trailing newline that similar adds
                let val = change.value();
                let line = if val.ends_with('\n') { &val[..val.len()-1] } else { val };
                ins_lines.push(line.to_string());
            }
        }
    }
    flush_eq(&mut ops, &mut eq_count);
    flush_del(&mut ops, &mut del_count);
    flush_ins(&mut ops, &mut ins_lines);

    serde_json::json!(ops)
}

/// Convert i32 to base-36 string, matching JS `Number.prototype.toString(36)`.
/// Negative numbers are prefixed with '-'.
fn to_base36(n: i32) -> String {
    if n == 0 {
        return "0".to_string();
    }
    let negative = n < 0;
    let mut val = if negative { (n as i64).abs() as u64 } else { n as u64 };
    let digits = b"0123456789abcdefghijklmnopqrstuvwxyz";
    let mut buf = Vec::new();
    while val > 0 {
        buf.push(digits[(val % 36) as usize]);
        val /= 36;
    }
    if negative {
        buf.push(b'-');
    }
    buf.reverse();
    String::from_utf8(buf).unwrap()
}

pub struct SyncEngine {
    api: ApiClient,
    local_path: PathBuf,
    local_hashes: HashMap<String, String>,
    local_content_cache: HashMap<String, String>,
    remote_modified: HashMap<String, String>,
}

impl SyncEngine {
    pub fn new(config: &Config) -> Self {
        Self {
            api: ApiClient::new(&config.api_base, &config.username, &config.api_token),
            local_path: PathBuf::from(&config.local_path),
            local_hashes: HashMap::new(),
            local_content_cache: HashMap::new(),
            remote_modified: HashMap::new(),
        }
    }

    pub fn simple_hash(s: &str) -> String {
        let mut hash: i32 = 0;
        for c in s.chars() {
            hash = ((hash << 5).wrapping_sub(hash)).wrapping_add(c as i32);
        }
        // JS의 hash.toString(36)과 동일한 base-36 출력
        to_base36(hash)
    }

    fn flatten_files(items: &[FileItem]) -> Vec<(String, Option<String>)> {
        let mut result = Vec::new();
        for item in items {
            if item.file_type == "folder" {
                if let Some(children) = &item.children {
                    result.extend(Self::flatten_files(children));
                }
            } else if item.file_type == "file" {
                result.push((item.path.clone(), item.modified.clone()));
            }
        }
        result
    }

    fn scan_local_md_files(&self) -> Vec<String> {
        flatten_file_paths(&scan_local_md_files(&self.local_path))
            .iter()
            .map(|item| item.path.clone())
            .collect()
    }

    pub fn full_sync(&mut self) -> Result<(usize, usize), Box<dyn std::error::Error>> {
        let mut downloaded = 0;
        let mut uploaded = 0;

        let remote_files = self.api.list_files()?;
        let remote_items = Self::flatten_files(&remote_files);
        let remote_paths: Vec<String> = remote_items.iter().map(|(p, _)| p.clone()).collect();

        let local_paths = self.scan_local_md_files();

        // 서버 → 로컬
        for (path, modified) in &remote_items {
            let local_file = self.local_path.join(path);
            let should_download = if !local_file.exists() {
                true
            } else if let Some(mod_time) = modified {
                self.remote_modified.get(path) != Some(mod_time)
            } else {
                false
            };

            if should_download {
                match self.api.get_file(path) {
                    Ok(content) => {
                        if let Some(parent) = local_file.parent() {
                            fs::create_dir_all(parent).ok();
                        }
                        if let Err(e) = fs::write(&local_file, &content.content) {
                            log::error!("파일 쓰기 실패 {}: {}", path, e);
                            continue;
                        }
                        self.local_hashes.insert(path.clone(), Self::simple_hash(&content.content));
                        self.local_content_cache.insert(path.clone(), content.content);
                        if let Some(mod_time) = modified {
                            self.remote_modified.insert(path.clone(), mod_time.clone());
                        }
                        println!("⬇️ {}", path);
                        downloaded += 1;
                    }
                    Err(e) => log::error!("파일 다운로드 실패 {}: {}", path, e),
                }
            }
        }

        // 로컬 → 서버
        for path in &local_paths {
            if !remote_paths.contains(path) {
                let local_file = self.local_path.join(path);
                match fs::read_to_string(&local_file) {
                    Ok(content) => {
                        if let Err(e) = self.api.put_file(path, &content) {
                            log::error!("파일 업로드 실패 {}: {}", path, e);
                            continue;
                        }
                        self.local_hashes.insert(path.clone(), Self::simple_hash(&content));
                        self.local_content_cache.insert(path.clone(), content);
                        println!("⬆️ {}", path);
                        uploaded += 1;
                    }
                    Err(e) => log::error!("파일 읽기 실패 {}: {}", path, e),
                }
            }
        }

        self.api.put_heartbeat();
        Ok((downloaded, uploaded))
    }

    pub fn handle_local_change(&mut self, full_path: &Path) {
        if let Ok(rel) = full_path.strip_prefix(&self.local_path) {
            let rel_str = rel.to_string_lossy().replace('\\', "/");

            if full_path.exists() {
                if let Ok(content) = fs::read_to_string(full_path) {
                    let new_hash = Self::simple_hash(&content);
                    if self.local_hashes.get(&rel_str) != Some(&new_hash) {
                        let old_hash = self.local_hashes.get(&rel_str).cloned();
                        // 이전 내용 읽어서 diff 생성 (해시가 있으면 이전 버전 존재)
                        let diff = if old_hash.is_some() {
                            let diff_val = generate_line_diff(
                                &self.local_content_cache.get(&rel_str).map(|s| s.as_str()).unwrap_or(""),
                                &content,
                            );
                            let diff_str = diff_val.to_string();
                            if diff_str.len() <= 10240 { Some(diff_val) } else { None }
                        } else {
                            None
                        };
                        self.local_hashes.insert(rel_str.clone(), new_hash);
                        self.local_content_cache.insert(rel_str.clone(), content.clone());
                        let result = self.api.put_file_with_diff(
                            &rel_str,
                            &content,
                            old_hash.as_deref(),
                            diff.as_ref(),
                        );
                        if result.is_ok() {
                            println!("⬆️ {}", rel_str);
                        }
                    }
                }
            } else {
                if self.api.delete_file(&rel_str).is_ok() {
                    self.local_hashes.remove(&rel_str);
                    self.local_content_cache.remove(&rel_str);
                    println!("🗑️ {}", rel_str);
                }
            }
        }
    }

    pub fn handle_local_folder_delete(&mut self, folder_path: &Path) {
        if let Ok(rel) = folder_path.strip_prefix(&self.local_path) {
            let prefix = rel.to_string_lossy().replace('\\', "/");
            let prefix_with_slash = if prefix.ends_with('/') { prefix.clone() } else { format!("{}/", prefix) };
            let to_delete: Vec<String> = self.local_hashes.keys()
                .filter(|k| k.starts_with(&prefix_with_slash))
                .cloned()
                .collect();
            for path in to_delete {
                if self.api.delete_file(&path).is_ok() {
                    self.local_hashes.remove(&path);
                    self.local_content_cache.remove(&path);
                    println!("🗑️ {}", path);
                }
            }
        }
    }

    /// Handle an RTDB event (from SSE subscription)
    pub fn handle_rtdb_event(&mut self, entry: &RtdbFileEntry) {
        match entry.action.as_str() {
            "save" => {
                let local_file = self.local_path.join(&entry.path);
                let local_hash = self.local_hashes.get(&entry.path).cloned();

                // diff 적용 가능: 로컬 해시 == oldHash
                if let (Some(old_hash), Some(diff), Some(ref lh)) = (&entry.old_hash, &entry.diff, &local_hash) {
                    if lh == old_hash {
                        if let Ok(old_content) = fs::read_to_string(&local_file) {
                            if let Some(new_content) = apply_line_diff(&old_content, diff) {
                                if let Some(parent) = local_file.parent() {
                                    fs::create_dir_all(parent).ok();
                                }
                                if fs::write(&local_file, &new_content).is_ok() {
                                    let hash = Self::simple_hash(&new_content);
                                    self.local_hashes.insert(entry.path.clone(), hash);
                                    self.local_content_cache.insert(entry.path.clone(), new_content);
                                    println!("⬇️ {} (diff applied)", entry.path);
                                    return;
                                }
                            }
                        }
                    }
                }

                // fallback: R2에서 전체 파일 fetch
                self.fetch_from_r2(&entry.path);
            }
            "create" => {
                self.fetch_from_r2(&entry.path);
            }
            "delete" => {
                let local_file = self.local_path.join(&entry.path);
                if local_file.exists() {
                    if fs::remove_file(&local_file).is_ok() {
                        self.local_hashes.remove(&entry.path);
                        self.local_content_cache.remove(&entry.path);
                        println!("🗑️ {} (rtdb)", entry.path);
                    }
                }
            }
            "rename" => {
                if let Some(old_path) = &entry.old_path {
                    let old_file = self.local_path.join(old_path);
                    let new_file = self.local_path.join(&entry.path);
                    if old_file.exists() {
                        if let Some(parent) = new_file.parent() {
                            fs::create_dir_all(parent).ok();
                        }
                        if fs::rename(&old_file, &new_file).is_ok() {
                            // 해시 이전
                            if let Some(h) = self.local_hashes.remove(old_path) {
                                self.local_hashes.insert(entry.path.clone(), h);
                            }
                            if let Some(c) = self.local_content_cache.remove(old_path) {
                                self.local_content_cache.insert(entry.path.clone(), c);
                            }
                            println!("📝 {} → {} (rtdb)", old_path, entry.path);
                        }
                    } else {
                        // 이전 파일 없으면 R2에서 fetch
                        self.fetch_from_r2(&entry.path);
                    }
                }
            }
            _ => {}
        }
    }

    fn fetch_from_r2(&mut self, path: &str) {
        match self.api.get_file(path) {
            Ok(content) => {
                let local_file = self.local_path.join(path);
                if let Some(parent) = local_file.parent() {
                    fs::create_dir_all(parent).ok();
                }
                if fs::write(&local_file, &content.content).is_ok() {
                    self.local_hashes.insert(path.to_string(), Self::simple_hash(&content.content));
                    self.local_content_cache.insert(path.to_string(), content.content);
                    println!("⬇️ {} (r2)", path);
                }
            }
            Err(e) => log::error!("R2 fetch 실패 {}: {}", path, e),
        }
    }
}

/// Start RTDB SSE subscription in a background thread.
/// Parses Firebase REST SSE events and dispatches to SyncEngine.
pub fn start_rtdb_subscription(
    rtdb_url: String,
    rtdb_auth: String,
    username: String,
    engine: Arc<Mutex<SyncEngine>>,
) {
    thread::spawn(move || {
        let client = reqwest::blocking::Client::builder()
            .timeout(None)
            .build()
            .unwrap();

        loop {
            let url = format!(
                "{}/mdflare/{}/files.json?auth={}",
                rtdb_url, username, rtdb_auth
            );
            println!("🔌 RTDB SSE 연결 중...");

            let resp = client
                .get(&url)
                .header("Accept", "text/event-stream")
                .send();

            match resp {
                Ok(response) => {
                    use std::io::{BufRead, BufReader};
                    let reader = BufReader::new(response);
                    let mut event_type = String::new();
                    let mut data_buf = String::new();
                    let mut first_put = true; // 첫 "put"은 전체 스냅샷 (무시)

                    println!("✅ RTDB SSE 연결됨");

                    for line in reader.lines() {
                        match line {
                            Ok(line) => {
                                if line.starts_with("event:") {
                                    event_type = line[6..].trim().to_string();
                                } else if line.starts_with("data:") {
                                    data_buf = line[5..].trim().to_string();
                                } else if line.is_empty() && !event_type.is_empty() {
                                    // 이벤트 완료 → 처리
                                    if event_type == "put" || event_type == "patch" {
                                        if first_put && event_type == "put" {
                                            first_put = false;
                                            // 첫 put은 전체 스냅샷, 스킵
                                            event_type.clear();
                                            data_buf.clear();
                                            continue;
                                        }
                                        handle_sse_data(&data_buf, &engine);
                                    } else if event_type == "keep-alive" {
                                        // ignore
                                    }
                                    event_type.clear();
                                    data_buf.clear();
                                }
                            }
                            Err(e) => {
                                eprintln!("⚠️ RTDB SSE 읽기 오류: {}", e);
                                break;
                            }
                        }
                    }

                    eprintln!("⚠️ RTDB SSE 연결 끊어짐, 5초 후 재연결...");
                }
                Err(e) => {
                    eprintln!("⚠️ RTDB SSE 연결 실패: {}, 5초 후 재시도...", e);
                }
            }

            thread::sleep(Duration::from_secs(5));
        }
    });
}

/// Parse SSE data payload and dispatch to SyncEngine
fn handle_sse_data(data: &str, engine: &Arc<Mutex<SyncEngine>>) {
    // Firebase SSE data format: {"path":"/safeKey","data":{...}} or {"path":"/","data":{...}}
    let parsed: Result<serde_json::Value, _> = serde_json::from_str(data);
    let val = match parsed {
        Ok(v) => v,
        Err(_) => return,
    };

    let path = val.get("path").and_then(|p| p.as_str()).unwrap_or("");
    let data_val = match val.get("data") {
        Some(d) => d,
        None => return,
    };

    if path == "/" {
        // 루트 업데이트: 여러 파일 변경 가능 (각 키가 safeKey)
        if let Some(obj) = data_val.as_object() {
            for (_key, entry_val) in obj {
                if let Ok(entry) = serde_json::from_value::<RtdbFileEntry>(entry_val.clone()) {
                    if let Ok(mut eng) = engine.lock() {
                        eng.handle_rtdb_event(&entry);
                    }
                }
            }
        }
    } else {
        // 개별 파일 변경: path = "/safeKey"
        if data_val.is_null() {
            // 삭제 이벤트: safeKey → path 복원
            let safe_key = path.trim_start_matches('/');
            let file_path = safe_key
                .replace("_slash_", "/")
                .replace("_dot_", ".");
            let entry = RtdbFileEntry {
                path: file_path,
                action: "delete".to_string(),
                hash: None,
                old_hash: None,
                diff: None,
                old_path: None,
                modified: None,
                size: None,
            };
            if let Ok(mut eng) = engine.lock() {
                eng.handle_rtdb_event(&entry);
            }
        } else if let Ok(entry) = serde_json::from_value::<RtdbFileEntry>(data_val.clone()) {
            if let Ok(mut eng) = engine.lock() {
                eng.handle_rtdb_event(&entry);
            }
        }
    }
}

pub fn start_cloud_sync(config: &Config) -> Arc<Mutex<SyncEngine>> {
    let engine = Arc::new(Mutex::new(SyncEngine::new(config)));
    let local_path = config.local_path.clone();

    // 파일 감시
    let engine_watcher = engine.clone();
    let watch_path = local_path.clone();
    thread::spawn(move || {
        let (tx, rx) = std::sync::mpsc::channel();
        let mut debouncer = new_debouncer(Duration::from_secs(1), tx).unwrap();
        debouncer.watcher().watch(Path::new(&watch_path), RecursiveMode::Recursive).ok();
        for events in rx.iter().flatten() {
            for event in events {
                if event.kind == DebouncedEventKind::Any {
                    if event.path.extension().map_or(false, |e| e == "md") {
                        if let Ok(mut eng) = engine_watcher.lock() {
                            eng.handle_local_change(&event.path);
                        }
                    } else if !event.path.exists() {
                        if let Ok(mut eng) = engine_watcher.lock() {
                            eng.handle_local_folder_delete(&event.path);
                        }
                    }
                }
            }
        }
    });

    // RTDB SSE 구독 (실시간 변경 감지)
    let engine_rtdb = engine.clone();
    let config_for_rtdb = config.clone();
    thread::spawn(move || {
        // sync-config에서 RTDB 접속 정보 가져오기
        let api = ApiClient::new(
            &config_for_rtdb.api_base,
            &config_for_rtdb.username,
            &config_for_rtdb.api_token,
        );
        match api.get_sync_config() {
            Ok(rtdb_config) => {
                println!("🔌 RTDB 접속 정보 수신: {}", rtdb_config.user_id);
                start_rtdb_subscription(
                    rtdb_config.rtdb_url,
                    rtdb_config.rtdb_auth,
                    rtdb_config.user_id,
                    engine_rtdb,
                );
            }
            Err(e) => {
                eprintln!("⚠️ RTDB 접속 정보 조회 실패: {} (폴링만 사용)", e);
            }
        }
    });

    // 주기적 동기화 (fallback: RTDB 연결 끊김 대비)
    let engine_timer = engine.clone();
    thread::spawn(move || {
        loop {
            thread::sleep(Duration::from_secs(30));
            if let Ok(mut eng) = engine_timer.lock() {
                eng.full_sync().ok();
            }
        }
    });

    // 초기 동기화
    if let Ok(mut eng) = engine.lock() {
        match eng.full_sync() {
            Ok((d, u)) => println!("✅ 초기 동기화 완료: ⬇️{} ⬆️{}", d, u),
            Err(e) => eprintln!("❌ 동기화 실패: {}", e),
        }
    }

    engine
}
