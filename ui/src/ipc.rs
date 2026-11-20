use iced::futures::{Stream, StreamExt};
use iced::stream;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;

/// 로컬 Go 데몬 Unix 도메인 소켓 경로를 결정한다.
pub fn socket_path() -> PathBuf {
    if let Ok(runtime_dir) = std::env::var("XDG_RUNTIME_DIR")
        && !runtime_dir.is_empty()
    {
        let p = PathBuf::from(runtime_dir).join("gemini-remote.sock");
        if p.exists() {
            return p;
        }
    }
    if let Ok(cfg_home) = std::env::var("XDG_CONFIG_HOME")
        && !cfg_home.is_empty()
    {
        return PathBuf::from(cfg_home)
            .join("gemini-remote")
            .join("daemon.sock");
    }
    if let Ok(home) = std::env::var("HOME") {
        return PathBuf::from(home)
            .join(".config")
            .join("gemini-remote")
            .join("daemon.sock");
    }
    PathBuf::from("/tmp/gemini-remote.sock")
}

#[allow(dead_code)]
#[derive(Debug, Clone, Deserialize)]
pub struct HealthResponse {
    pub ok: bool,
    #[serde(default)]
    pub device_name: String,
    #[serde(default)]
    pub device_id: String,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Default, Deserialize)]
pub struct UserProfile {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub given_name: String,
    #[serde(default)]
    pub email: String,
    #[serde(default)]
    pub picture: String,
    #[serde(default)]
    pub tier: String,
    #[serde(default)]
    pub device_name: String,
    #[serde(default)]
    pub device_id: String,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct BardModeOption {
    pub id: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub title_for_picker: String,
    #[serde(default)]
    pub description_for_picker: String,
    #[serde(default)]
    pub mode_name: String,
    #[serde(default)]
    pub full_mode_name: String,
    #[serde(default)]
    pub bard_mode: i32,
    #[serde(default)]
    pub mode_category: i32,
    #[serde(default)]
    pub in_picker: bool,
    #[serde(default)]
    pub requires_advanced: bool,
    #[serde(default)]
    pub supports_thinking: bool,
}

impl BardModeOption {
    /// 입력창 툴바 필 버튼에 표시할 간결한 모델 이름.
    pub fn pill_label(&self) -> &str {
        if !self.title.is_empty() {
            &self.title
        } else if !self.mode_name.is_empty() {
            &self.mode_name
        } else if !self.title_for_picker.is_empty() {
            &self.title_for_picker
        } else {
            &self.id
        }
    }

    /// 팝오버 메뉴 항목 상단에 표시할 주요 제목 (원본 앱: 용도 설명 우선).
    pub fn picker_heading(&self) -> &str {
        if !self.title_for_picker.is_empty() {
            &self.title_for_picker
        } else if !self.title.is_empty() {
            &self.title
        } else {
            &self.id
        }
    }

    /// 팝오버 메뉴 항목 하단에 표시할 보조 모델명/설명 (원본 앱: 4 Hydrogen / 4 Neon / 4 카본 등).
    pub fn picker_subtitle(&self) -> String {
        let heading = self.picker_heading().trim();
        let desc = self.description_for_picker.trim();
        if !desc.is_empty() && desc != heading {
            return desc.to_string();
        }
        let title = self.title.trim();
        if !title.is_empty() && title != heading {
            return title.to_string();
        }
        let fallback_desc = self.description.trim();
        if !fallback_desc.is_empty() && fallback_desc != heading {
            fallback_desc.to_string()
        } else {
            String::new()
        }
    }
}

#[allow(dead_code)]
#[derive(Debug, Clone, Deserialize)]
pub struct ModelsResponse {
    #[serde(default)]
    pub mode_picker_title: String,
    #[serde(default)]
    pub tier: String,
    #[serde(default)]
    pub models: Vec<BardModeOption>,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Deserialize)]
pub struct BotSummary {
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub starters: Vec<String>,
    #[serde(default)]
    pub is_custom: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct BotsResponse {
    #[serde(default)]
    pub bots: Vec<BotSummary>,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Deserialize)]
pub struct RegisteredDevice {
    #[serde(rename = "DeviceID", default)]
    pub device_id: String,
    #[serde(rename = "DeviceName", default)]
    pub device_name: String,
    #[serde(rename = "RemoteEnabled", default)]
    pub remote_enabled: bool,
    #[serde(rename = "ToolCount", default)]
    pub tool_count: usize,
    #[serde(rename = "Folders", default)]
    pub folders: Option<Vec<String>>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DevicesResponse {
    #[serde(default)]
    pub devices: Vec<RegisteredDevice>,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Default, Deserialize)]
pub struct RemoteMCPServerStatus {
    #[serde(default)]
    pub server_id: String,
    #[serde(default)]
    pub display_name: String,
    #[serde(default)]
    pub enabled: bool,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Default, Deserialize)]
pub struct ToolExecEvent {
    #[serde(default)]
    pub session_id: String,
    #[serde(default)]
    pub call_id: String,
    #[serde(default)]
    pub tool_name: String,
    #[serde(default)]
    pub args_brief: String,
    #[serde(default)]
    pub phase: String,
    #[serde(default)]
    pub error: String,
    #[serde(default)]
    pub duration_ms: i64,
    #[serde(default)]
    pub timestamp_unix: i64,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Default, Deserialize)]
pub struct RemoteStatusSnapshot {
    #[serde(default)]
    pub device_id: String,
    #[serde(default)]
    pub device_name: String,
    #[serde(default)]
    pub remote_enabled: bool,
    #[serde(default)]
    pub punctual_connected: bool,
    #[serde(default)]
    pub active_sessions: Vec<String>,
    #[serde(default)]
    pub current_tool: String,
    #[serde(default)]
    pub current_tool_args: String,
    #[serde(default)]
    pub tool_count: usize,
    #[serde(default)]
    pub connected_folders: Vec<String>,
    #[serde(default)]
    pub mcp_servers: Vec<RemoteMCPServerStatus>,
    #[serde(default)]
    pub recent_events: Vec<ToolExecEvent>,
    #[serde(default)]
    pub last_sync_unix: i64,
}

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub enum RemoteEvent {
    Status(RemoteStatusSnapshot),
    Tool(ToolExecEvent),
}

#[allow(dead_code)]
#[derive(Debug, Clone, Deserialize)]
pub struct ConversationSummary {
    pub conversation_id: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub pinned: bool,
    #[serde(default)]
    pub is_agent_chat: bool,
    #[serde(default)]
    pub has_unread: bool,
    #[serde(default)]
    pub timestamp_unix: i64,
    #[serde(default)]
    pub bot_id: String,
    #[serde(default)]
    pub project_id: String,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Deserialize)]
pub struct ListConversationsResponse {
    #[serde(default)]
    pub conversations: Vec<ConversationSummary>,
    #[serde(default)]
    pub next_page_token: String,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Deserialize)]
pub struct SearchHit {
    pub conversation_id: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub snippet: String,
    #[serde(default)]
    pub request_id: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SearchResponse {
    #[serde(default)]
    pub hits: Vec<SearchHit>,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Notebook {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub sources: Vec<String>,
    #[serde(default)]
    pub notes: String,
    #[serde(default)]
    pub source_count: usize,
    #[serde(default)]
    pub is_remote: bool,
    #[serde(default)]
    pub conversation_ids: Vec<String>,
    #[serde(default)]
    pub updated_at_unix: i64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct NotebooksResponse {
    #[serde(default)]
    pub notebooks: Vec<Notebook>,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ScheduledTask {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub prompt: String,
    #[serde(default)]
    pub schedule_type: String,
    #[serde(default)]
    pub time_of_day: String,
    #[serde(default)]
    pub schedule_desc: String,
    #[serde(default)]
    pub is_remote: bool,
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub last_run_unix: i64,
    #[serde(default)]
    pub last_conversation_id: String,
    #[serde(default)]
    pub last_status: String,
    #[serde(default)]
    pub last_excerpt: String,
    #[serde(default)]
    pub created_at_unix: i64,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Default, Deserialize)]
pub struct RecommendedSchedule {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub prompt: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SchedulesResponse {
    #[serde(default)]
    pub schedules: Vec<ScheduledTask>,
    #[serde(default)]
    pub recommended: Vec<RecommendedSchedule>,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Deserialize)]
pub struct ScheduleRunResult {
    #[serde(default)]
    pub ok: bool,
    #[serde(default)]
    pub conversation_id: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub excerpt: String,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Deserialize)]
pub struct LibraryEntry {
    pub id: String,
    #[serde(default)]
    pub kind: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub path: String,
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub preview: String,
    #[serde(default)]
    pub content_preview: String,
    #[serde(default)]
    pub is_remote: bool,
    #[serde(default)]
    pub conversation_id: String,
    #[serde(default)]
    pub created_at_unix: i64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LibraryResponse {
    #[serde(default)]
    pub items: Vec<LibraryEntry>,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Default, Deserialize)]
pub struct UsageBucket {
    #[serde(default)]
    pub bucket_type: i32,
    #[serde(default)]
    pub label: String,
    #[serde(default)]
    pub used_ratio: f32,
    #[serde(default)]
    pub reset_at_unix: i64,
    #[serde(default)]
    pub remaining_val: i64,
    #[serde(default)]
    pub max_val: i64,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Default, Deserialize)]
pub struct UsageInfo {
    #[serde(default)]
    pub buckets: Vec<UsageBucket>,
    #[serde(default)]
    pub notice_text: String,
    #[serde(default)]
    pub support_url: String,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Default, Deserialize)]
pub struct UIPreferences {
    #[serde(default)]
    pub selected_model_id: String,
    #[serde(default)]
    pub thinking_level: i32,
    #[serde(default)]
    pub theme_mode: String,
    #[serde(default)]
    pub language: String,
    #[serde(default)]
    pub personal_intelligence: bool,
    #[serde(default)]
    pub custom_instructions: String,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Default, Deserialize)]
pub struct ShareResponse {
    #[serde(default)]
    pub ok: bool,
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub remote: bool,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Default, Deserialize)]
pub struct TurnAttachment {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub mime_type: String,
    #[serde(default)]
    pub width: i32,
    #[serde(default)]
    pub height: i32,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Default, Deserialize)]
pub struct GeneratedImage {
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub placeholder_id: String,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Default, Deserialize)]
pub struct TurnArtifact {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub language: String,
    #[serde(default)]
    pub content: String,
    #[serde(default)]
    pub url: String,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Deserialize)]
pub struct ConversationTurn {
    #[serde(default)]
    pub conversation_id: String,
    #[serde(default)]
    pub request_id: String,
    #[serde(default)]
    pub response_id: String,
    #[serde(default)]
    pub user_text: String,
    #[serde(default)]
    pub user_attachments: Vec<TurnAttachment>,
    #[serde(default)]
    pub model_text: String,
    #[serde(default)]
    pub thoughts: String,
    #[serde(default)]
    pub status_text: String,
    #[serde(default)]
    pub tool_activities: Vec<RemoteToolActivity>,
    #[serde(default)]
    pub generated_images: Vec<GeneratedImage>,
    #[serde(default)]
    pub artifacts: Vec<TurnArtifact>,
    #[serde(default)]
    pub is_scheduled_run: bool,
    #[serde(default)]
    pub scheduled_title: String,
    #[serde(default)]
    pub model_attribution: String,
    #[serde(default)]
    pub aborted: bool,
    #[serde(default)]
    pub timestamp_unix: i64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ListTurnsResponse {
    #[serde(default)]
    pub turns: Vec<ConversationTurn>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ChatStreamRequest {
    pub text: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub conversation_id: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub parent_request_id: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub parent_response_id: String,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub temporary: bool,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub bot_id: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub project_id: String,
    #[serde(skip_serializing_if = "is_zero_i32")]
    pub mode_category: i32,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub mode_variant_id: String,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub skip_thinking: bool,
    #[serde(skip_serializing_if = "is_zero_i32")]
    pub thinking_level: i32,
    #[serde(skip_serializing_if = "is_zero_i32")]
    pub feature_mode: i32,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub personal_intelligence: bool,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub agent_task: bool,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub target_device_id: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub target_device_name: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub file_paths: Vec<String>,
}

fn is_zero_i32(v: &i32) -> bool {
    *v == 0
}

#[allow(dead_code)]
#[derive(Debug, Clone, Default, Deserialize, PartialEq, Eq)]
pub struct RemoteToolActivity {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub action: String,
    #[serde(default)]
    pub summary: String,
    #[serde(default)]
    pub explanation: String,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Deserialize)]
pub struct ChatStreamChunk {
    #[serde(rename = "type", default)]
    pub kind: String,
    #[serde(default)]
    pub conversation_id: String,
    #[serde(default)]
    pub request_id: String,
    #[serde(default)]
    pub response_id: String,
    #[serde(default)]
    pub text: String,
    #[serde(default)]
    pub delta: String,
    #[serde(default)]
    pub thoughts: String,
    #[serde(default)]
    pub thoughts_delta: String,
    #[serde(default)]
    pub status_text: String,
    #[serde(default)]
    pub tool_activities: Vec<RemoteToolActivity>,
    #[serde(default)]
    pub artifacts: Vec<TurnArtifact>,
    #[serde(default)]
    pub target_device_name: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub model: String,
    #[serde(default)]
    pub done: bool,
}

#[derive(Debug, Clone)]
pub enum StreamEvent {
    Chunk(ChatStreamChunk),
    Done(ChatStreamChunk),
    Error(String),
}

fn url_encode_query(s: &str) -> String {
    let mut out = String::with_capacity(s.len() * 3);
    for b in s.as_bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(*b as char);
            }
            b' ' => out.push_str("%20"),
            _ => out.push_str(&format!("%{:02X}", b)),
        }
    }
    out
}

async fn send_http_request(
    method: &str,
    path: &str,
    body: Option<&[u8]>,
) -> Result<(u16, bool, Option<usize>, BufReader<UnixStream>), String> {
    let sock = socket_path();
    let mut stream = UnixStream::connect(&sock)
        .await
        .map_err(|e| format!("데몬 소켓 연결 실패 ({}): {e}", sock.display()))?;

    let payload = body.unwrap_or(&[]);
    let mut req = format!(
        "{method} {path} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n"
    );
    if body.is_some() {
        req.push_str(&format!(
            "Content-Type: application/json\r\nContent-Length: {}\r\n",
            payload.len()
        ));
    }
    req.push_str("\r\n");

    stream
        .write_all(req.as_bytes())
        .await
        .map_err(|e| format!("요청 헤더 전송 실패: {e}"))?;
    if !payload.is_empty() {
        stream
            .write_all(payload)
            .await
            .map_err(|e| format!("요청 본문 전송 실패: {e}"))?;
    }
    stream
        .flush()
        .await
        .map_err(|e| format!("소켓 플러시 실패: {e}"))?;

    let mut reader = BufReader::new(stream);
    let mut status_line = String::new();
    reader
        .read_line(&mut status_line)
        .await
        .map_err(|e| format!("응답 상태 읽기 실패: {e}"))?;
    let status: u16 = status_line
        .split_whitespace()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .ok_or_else(|| format!("잘못된 HTTP 상태 줄: {status_line}"))?;

    let mut chunked = false;
    let mut content_len = None;
    loop {
        let mut line = String::new();
        let n = reader
            .read_line(&mut line)
            .await
            .map_err(|e| format!("응답 헤더 읽기 실패: {e}"))?;
        if n == 0 || line == "\r\n" || line == "\n" {
            break;
        }
        if let Some((k, v)) = line.split_once(':') {
            let kl = k.trim().to_ascii_lowercase();
            let vl = v.trim();
            if kl == "transfer-encoding" && vl.to_ascii_lowercase().contains("chunked") {
                chunked = true;
            } else if kl == "content-length" {
                content_len = vl.parse::<usize>().ok();
            }
        }
    }

    Ok((status, chunked, content_len, reader))
}

async fn read_body_bytes(
    chunked: bool,
    content_len: Option<usize>,
    reader: &mut BufReader<UnixStream>,
) -> Result<Vec<u8>, String> {
    if chunked {
        let mut out = Vec::new();
        loop {
            let mut size_line = String::new();
            let n = reader
                .read_line(&mut size_line)
                .await
                .map_err(|e| format!("chunk 크기 읽기 실패: {e}"))?;
            if n == 0 {
                break;
            }
            let hex = size_line.trim().split(';').next().unwrap_or("0");
            let sz = usize::from_str_radix(hex, 16)
                .map_err(|e| format!("잘못된 chunk 크기 ({hex}): {e}"))?;
            if sz == 0 {
                let mut trail = String::new();
                let _ = reader.read_line(&mut trail).await;
                break;
            }
            let mut buf = vec![0u8; sz];
            reader
                .read_exact(&mut buf)
                .await
                .map_err(|e| format!("chunk 본문 읽기 실패: {e}"))?;
            out.extend_from_slice(&buf);
            let mut crlf = [0u8; 2];
            let _ = reader.read_exact(&mut crlf).await;
        }
        Ok(out)
    } else if let Some(len) = content_len {
        let mut buf = vec![0u8; len];
        reader
            .read_exact(&mut buf)
            .await
            .map_err(|e| format!("응답 본문 읽기 실패: {e}"))?;
        Ok(buf)
    } else {
        let mut out = Vec::new();
        reader
            .read_to_end(&mut out)
            .await
            .map_err(|e| format!("응답 본문 EOF 읽기 실패: {e}"))?;
        Ok(out)
    }
}

async fn request_bytes(method: &str, path: &str, body: Option<&[u8]>) -> Result<Vec<u8>, String> {
    let (status, chunked, content_len, mut reader) = send_http_request(method, path, body).await?;
    let bytes = read_body_bytes(chunked, content_len, &mut reader).await?;
    if status >= 400 {
        let text = String::from_utf8_lossy(&bytes);
        return Err(format!("HTTP {status}: {text}"));
    }
    Ok(bytes)
}

async fn request_json<T: for<'de> Deserialize<'de>>(
    method: &str,
    path: &str,
    body: Option<&[u8]>,
) -> Result<T, String> {
    let bytes = request_bytes(method, path, body).await?;
    serde_json::from_slice(&bytes).map_err(|e| format!("JSON 파싱 실패: {e}"))
}

pub async fn health() -> Result<HealthResponse, String> {
    request_json("GET", "/health", None).await
}

pub async fn get_profile() -> Result<UserProfile, String> {
    request_json("GET", "/v1/profile", None).await
}

pub async fn get_avatar_bytes() -> Result<Vec<u8>, String> {
    request_bytes("GET", "/v1/profile/avatar", None).await
}

pub async fn list_models() -> Result<ModelsResponse, String> {
    request_json("GET", "/v1/models", None).await
}

pub async fn list_bots() -> Result<BotsResponse, String> {
    request_json("GET", "/v1/bots", None).await
}

pub async fn list_devices() -> Result<DevicesResponse, String> {
    request_json("GET", "/v1/devices", None).await
}

pub async fn get_remote_status() -> Result<RemoteStatusSnapshot, String> {
    request_json("GET", "/v1/remote/status", None).await
}

pub async fn toggle_remote(enabled: bool) -> Result<RemoteStatusSnapshot, String> {
    let body = serde_json::to_vec(&serde_json::json!({ "enabled": enabled }))
        .map_err(|e| e.to_string())?;
    request_json("POST", "/v1/remote/toggle", Some(&body)).await
}

pub async fn register_remote() -> Result<RemoteStatusSnapshot, String> {
    request_json("POST", "/v1/remote/register", Some(b"{}")).await
}

pub async fn add_remote_folder(path: String) -> Result<RemoteStatusSnapshot, String> {
    let body = serde_json::to_vec(&serde_json::json!({ "path": path }))
        .map_err(|e| e.to_string())?;
    request_json("POST", "/v1/remote/folders", Some(&body)).await
}

pub async fn remove_remote_folder(path: String) -> Result<RemoteStatusSnapshot, String> {
    let body = serde_json::to_vec(&serde_json::json!({ "path": path }))
        .map_err(|e| e.to_string())?;
    request_json("DELETE", "/v1/remote/folders", Some(&body)).await
}

pub async fn list_conversations(page_size: usize) -> Result<ListConversationsResponse, String> {
    request_json(
        "GET",
        &format!("/v1/conversations?page_size={page_size}"),
        None,
    )
    .await
}

pub async fn search_conversations(query: String) -> Result<SearchResponse, String> {
    let q = url_encode_query(query.trim());
    request_json("GET", &format!("/v1/conversations/search?q={q}"), None).await
}

pub async fn list_turns(conv_id: String, page_size: usize) -> Result<ListTurnsResponse, String> {
    request_json(
        "GET",
        &format!("/v1/conversations/{conv_id}/turns?page_size={page_size}"),
        None,
    )
    .await
}

pub async fn delete_conversation(conv_id: String) -> Result<String, String> {
    let _: serde_json::Value =
        request_json("DELETE", &format!("/v1/conversations/{conv_id}"), None).await?;
    Ok(conv_id)
}

pub async fn rename_conversation(conv_id: String, title: String) -> Result<(String, String), String> {
    let body = serde_json::to_vec(&serde_json::json!({ "title": title }))
        .map_err(|e| e.to_string())?;
    let _: serde_json::Value = request_json(
        "PATCH",
        &format!("/v1/conversations/{conv_id}"),
        Some(&body),
    )
    .await?;
    Ok((conv_id, title))
}

pub async fn pin_conversation(conv_id: String, pinned: bool) -> Result<(String, bool), String> {
    let body = serde_json::to_vec(&serde_json::json!({ "pinned": pinned }))
        .map_err(|e| e.to_string())?;
    let _: serde_json::Value = request_json(
        "PATCH",
        &format!("/v1/conversations/{conv_id}"),
        Some(&body),
    )
    .await?;
    Ok((conv_id, pinned))
}

pub async fn list_notebooks() -> Result<NotebooksResponse, String> {
    request_json("GET", "/v1/notebooks", None).await
}

pub async fn upsert_notebook(nb: Notebook) -> Result<Notebook, String> {
    let body = serde_json::to_vec(&nb).map_err(|e| e.to_string())?;
    request_json("POST", "/v1/notebooks", Some(&body)).await
}

pub async fn delete_notebook(id: String) -> Result<String, String> {
    let _: serde_json::Value =
        request_json("DELETE", &format!("/v1/notebooks/{id}"), None).await?;
    Ok(id)
}

pub async fn list_schedules() -> Result<SchedulesResponse, String> {
    request_json("GET", "/v1/schedules", None).await
}

pub async fn create_schedule(task: ScheduledTask) -> Result<ScheduledTask, String> {
    let body = serde_json::to_vec(&task).map_err(|e| e.to_string())?;
    request_json("POST", "/v1/schedules", Some(&body)).await
}

pub async fn patch_schedule(id: String, enabled: bool) -> Result<ScheduledTask, String> {
    let body = serde_json::to_vec(&serde_json::json!({ "enabled": enabled }))
        .map_err(|e| e.to_string())?;
    request_json("PATCH", &format!("/v1/schedules/{id}"), Some(&body)).await
}

pub async fn delete_schedule(id: String) -> Result<String, String> {
    let _: serde_json::Value =
        request_json("DELETE", &format!("/v1/schedules/{id}"), None).await?;
    Ok(id)
}

pub async fn run_schedule_now(id: String) -> Result<ScheduleRunResult, String> {
    request_json("POST", &format!("/v1/schedules/{id}/run"), Some(b"{}")).await
}

pub async fn list_library() -> Result<LibraryResponse, String> {
    request_json("GET", "/v1/library", None).await
}

pub async fn fetch_library_image(url: String) -> Result<Vec<u8>, String> {
    let q = url_encode_query(&url);
    request_bytes("GET", &format!("/v1/library/image?url={q}"), None).await
}

pub async fn get_preferences() -> Result<UIPreferences, String> {
    request_json("GET", "/v1/preferences", None).await
}

pub async fn save_preferences(patch: serde_json::Value) -> Result<UIPreferences, String> {
    let body = serde_json::to_vec(&patch).map_err(|e| e.to_string())?;
    request_json("PATCH", "/v1/preferences", Some(&body)).await
}

pub async fn get_usage() -> Result<UsageInfo, String> {
    request_json("GET", "/v1/usage", None).await
}

pub async fn share_conversation(conv_id: String, title: String) -> Result<ShareResponse, String> {
    let body = serde_json::to_vec(&serde_json::json!({ "title": title }))
        .map_err(|e| e.to_string())?;
    request_json(
        "POST",
        &format!("/v1/conversations/{conv_id}/share"),
        Some(&body),
    )
    .await
}

pub async fn mark_conversation_unread(
    conv_id: String,
    unread: bool,
) -> Result<(String, bool), String> {
    let body = serde_json::to_vec(&serde_json::json!({ "unread": unread }))
        .map_err(|e| e.to_string())?;
    let _: serde_json::Value = request_json(
        "PATCH",
        &format!("/v1/conversations/{conv_id}"),
        Some(&body),
    )
    .await?;
    Ok((conv_id, unread))
}

pub async fn send_feedback(
    conv_id: String,
    req_id: String,
    resp_id: String,
    positive: bool,
) -> Result<(), String> {
    let body = serde_json::to_vec(&serde_json::json!({
        "conversation_id": conv_id,
        "request_id": req_id,
        "response_id": resp_id,
        "positive": positive,
    }))
    .map_err(|e| e.to_string())?;
    let _: serde_json::Value = request_json("POST", "/v1/feedback", Some(&body)).await?;
    Ok(())
}

pub async fn add_recommended_schedule(
    id: String,
    title: String,
    description: String,
    prompt: String,
) -> Result<String, String> {
    let body = serde_json::to_vec(&serde_json::json!({
        "id": id,
        "title": title,
        "description": description,
        "prompt": prompt,
    }))
    .map_err(|e| e.to_string())?;
    let _: serde_json::Value =
        request_json("POST", "/v1/schedules/recommended", Some(&body)).await?;
    Ok(title)
}

pub async fn add_conversation_to_notebook(
    notebook_id: String,
    conv_id: String,
) -> Result<(), String> {
    let body = serde_json::to_vec(&serde_json::json!({ "conversation_id": conv_id }))
        .map_err(|e| e.to_string())?;
    let _: serde_json::Value = request_json(
        "POST",
        &format!("/v1/notebooks/{notebook_id}/conversations"),
        Some(&body),
    )
    .await?;
    Ok(())
}

pub async fn abort_generation(conv_id: String, req_id: String) -> Result<(), String> {
    let body = serde_json::to_vec(&serde_json::json!({
        "conversation_id": conv_id,
        "request_id": req_id,
    }))
    .map_err(|e| e.to_string())?;
    let _: serde_json::Value = request_json("POST", "/v1/chat/abort", Some(&body)).await?;
    Ok(())
}

/// 바이트 버퍼에서 `\n` 단위로 완성된 한 줄을 꺼내 UTF-8 문자열로 디코딩한다.
/// 청크 경계에서 멀티바이트 UTF-8 문자가 분할되어도 깨지지 않도록 바이트 단위로 누적한 뒤 디코딩한다.
fn take_next_utf8_line(pending: &mut Vec<u8>) -> Option<String> {
    let nl = pending.iter().position(|&b| b == b'\n')?;
    let mut raw: Vec<u8> = pending.drain(..=nl).collect();
    if raw.last() == Some(&b'\n') {
        raw.pop();
    }
    if raw.last() == Some(&b'\r') {
        raw.pop();
    }
    Some(String::from_utf8_lossy(&raw).into_owned())
}

/// Chunked 또는 Plain HTTP 바디에서 SSE 이벤트를 실시간 스트리밍으로 방출한다.
pub fn stream_chat(req: ChatStreamRequest) -> impl Stream<Item = StreamEvent> {
    stream::channel(64, move |mut tx: iced::futures::channel::mpsc::Sender<StreamEvent>| async move {
        let body = match serde_json::to_vec(&req) {
            Ok(b) => b,
            Err(e) => {
                let _ = tx.try_send(StreamEvent::Error(e.to_string()));
                return;
            }
        };

        let (status, chunked, _content_len, mut reader) =
            match send_http_request("POST", "/v1/chat/stream", Some(&body)).await {
                Ok(v) => v,
                Err(e) => {
                    let _ = tx.try_send(StreamEvent::Error(e));
                    return;
                }
            };

        if status >= 400 {
            let mut rest = Vec::new();
            let _ = reader.read_to_end(&mut rest).await;
            let _ = tx.try_send(StreamEvent::Error(format!(
                "HTTP {status}: {}",
                String::from_utf8_lossy(&rest)
            )));
            return;
        }

        let mut pending: Vec<u8> = Vec::new();
        let mut cur_event = String::new();
        let mut cur_data = String::new();

        loop {
            let piece: Vec<u8> = if chunked {
                let mut size_line = String::new();
                match reader.read_line(&mut size_line).await {
                    Ok(0) => break,
                    Ok(_) => {
                        let hex = size_line.trim().split(';').next().unwrap_or("0");
                        let sz = match usize::from_str_radix(hex, 16) {
                            Ok(v) => v,
                            Err(e) => {
                                let _ = tx.try_send(StreamEvent::Error(format!(
                                    "SSE chunk 크기 파싱 실패 ({hex}): {e}"
                                )));
                                break;
                            }
                        };
                        if sz == 0 {
                            break;
                        }
                        let mut buf = vec![0u8; sz];
                        if let Err(e) = reader.read_exact(&mut buf).await {
                            let _ = tx.try_send(StreamEvent::Error(format!(
                                "SSE chunk 읽기 실패: {e}"
                            )));
                            break;
                        }
                        let mut crlf = [0u8; 2];
                        let _ = reader.read_exact(&mut crlf).await;
                        buf
                    }
                    Err(e) => {
                        let _ = tx.try_send(StreamEvent::Error(format!("스트림 읽기 오류: {e}")));
                        break;
                    }
                }
            } else {
                let mut line = Vec::new();
                match reader.read_until(b'\n', &mut line).await {
                    Ok(0) => break,
                    Ok(_) => line,
                    Err(e) => {
                        let _ = tx.try_send(StreamEvent::Error(format!("스트림 읽기 오류: {e}")));
                        break;
                    }
                }
            };

            pending.extend_from_slice(&piece);
            while let Some(line) = take_next_utf8_line(&mut pending) {
                if line.is_empty() {
                    if !cur_data.is_empty() {
                        let ev = std::mem::take(&mut cur_event);
                        let data = std::mem::take(&mut cur_data);
                        if ev == "error" {
                            let msg = serde_json::from_str::<serde_json::Value>(&data)
                                .ok()
                                .and_then(|v| v.get("error")?.as_str().map(String::from))
                                .unwrap_or(data);
                            let _ = tx.try_send(StreamEvent::Error(msg));
                        } else if let Ok(chunk) = serde_json::from_str::<ChatStreamChunk>(&data) {
                            let item = if ev == "done" {
                                StreamEvent::Done(chunk)
                            } else {
                                StreamEvent::Chunk(chunk)
                            };
                            if tx.try_send(item).is_err() {
                                return;
                            }
                        }
                    } else {
                        cur_event.clear();
                    }
                } else if let Some(rest) = line.strip_prefix("event:") {
                    cur_event = rest.trim().to_string();
                } else if let Some(rest) = line.strip_prefix("data:") {
                    if !cur_data.is_empty() {
                        cur_data.push('\n');
                    }
                    cur_data.push_str(rest.trim());
                }
            }
        }
    })
    .boxed()
}

/// 데몬 원격 제어 상태 및 실시간 도구 실행 SSE(`/v1/remote/events`)를 자동 재연결하며 구독한다.
pub fn stream_remote_events() -> impl Stream<Item = RemoteEvent> {
    stream::channel(
        32,
        move |mut tx: iced::futures::channel::mpsc::Sender<RemoteEvent>| async move {
            loop {
                if let Ok((status, chunked, _content_len, mut reader)) =
                    send_http_request("GET", "/v1/remote/events", None).await
                    && status < 400
                {
                    let mut pending: Vec<u8> = Vec::new();
                    let mut cur_event = String::new();
                    let mut cur_data = String::new();

                    loop {
                        let piece: Vec<u8> = if chunked {
                            let mut size_line = String::new();
                            match reader.read_line(&mut size_line).await {
                                Ok(0) | Err(_) => break,
                                Ok(_) => {
                                    let hex = size_line.trim().split(';').next().unwrap_or("0");
                                    let Ok(sz) = usize::from_str_radix(hex, 16) else {
                                        break;
                                    };
                                    if sz == 0 {
                                        break;
                                    }
                                    let mut buf = vec![0u8; sz];
                                    if reader.read_exact(&mut buf).await.is_err() {
                                        break;
                                    }
                                    let mut crlf = [0u8; 2];
                                    let _ = reader.read_exact(&mut crlf).await;
                                    buf
                                }
                            }
                        } else {
                            let mut line = Vec::new();
                            match reader.read_until(b'\n', &mut line).await {
                                Ok(0) | Err(_) => break,
                                Ok(_) => line,
                            }
                        };

                        pending.extend_from_slice(&piece);
                        while let Some(line) = take_next_utf8_line(&mut pending) {
                            if line.is_empty() {
                                if !cur_data.is_empty() {
                                    let ev = std::mem::take(&mut cur_event);
                                    let data = std::mem::take(&mut cur_data);
                                    if ev == "status" {
                                        if let Ok(snap) =
                                            serde_json::from_str::<RemoteStatusSnapshot>(&data)
                                            && tx.try_send(RemoteEvent::Status(snap)).is_err()
                                        {
                                            return;
                                        }
                                    } else if ev == "tool"
                                        && let Ok(tool_ev) =
                                            serde_json::from_str::<ToolExecEvent>(&data)
                                        && tx.try_send(RemoteEvent::Tool(tool_ev)).is_err()
                                    {
                                        return;
                                    }
                                } else {
                                    cur_event.clear();
                                }
                            } else if let Some(rest) = line.strip_prefix("event:") {
                                cur_event = rest.trim().to_string();
                            } else if let Some(rest) = line.strip_prefix("data:") {
                                if !cur_data.is_empty() {
                                    cur_data.push('\n');
                                }
                                cur_data.push_str(rest.trim());
                            }
                        }
                    }
                }
                tokio::time::sleep(std::time::Duration::from_secs(3)).await;
            }
        },
    )
    .boxed()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn url_encode_query_handles_korean_and_spaces() {
        assert_eq!(url_encode_query("abc 123"), "abc%20123");
        assert_eq!(url_encode_query("가"), "%EA%B0%80");
    }

    #[test]
    fn 청크_경계_멀티바이트_utf8_줄_복원() {
        let raw = "data: {\"text\":\"한글 스트리밍\"}\r\n\r\n".as_bytes();
        // '한'(3바이트) 중간을 쪼개어 두 청크로 전달해도 손상 없이 복원되어야 한다.
        let split_at = 17;
        let mut pending = Vec::new();
        pending.extend_from_slice(&raw[..split_at]);
        assert!(take_next_utf8_line(&mut pending).is_none());

        pending.extend_from_slice(&raw[split_at..]);
        let line1 = take_next_utf8_line(&mut pending).expect("첫 줄이 추출되어야 함");
        assert_eq!(line1, "data: {\"text\":\"한글 스트리밍\"}");
        let line2 = take_next_utf8_line(&mut pending).expect("빈 구분 줄이 추출되어야 함");
        assert_eq!(line2, "");
    }
}
