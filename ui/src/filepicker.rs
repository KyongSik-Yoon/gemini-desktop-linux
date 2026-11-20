use iced::futures::StreamExt;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use zbus::zvariant::{ObjectPath, OwnedValue, Value};
use zbus::{Connection, Proxy};

static REQ_COUNTER: AtomicU64 = AtomicU64::new(1);

/// XDG Desktop Portal(`org.freedesktop.portal.FileChooser`)을 통해 네이티브 파일 선택기를 연다.
pub async fn pick_files() -> Result<Vec<PathBuf>, String> {
    let conn = Connection::session()
        .await
        .map_err(|e| format!("세션 D-Bus 연결 실패: {e}"))?;

    let unique = conn
        .unique_name()
        .ok_or_else(|| "D-Bus 고유 이름을 가져올 수 없습니다".to_string())?
        .as_str()
        .trim_start_matches(':')
        .replace('.', "_");

    let seq = REQ_COUNTER.fetch_add(1, Ordering::Relaxed);
    let token = format!("gemini_ui_{}_{seq}", std::process::id());
    let req_path = format!("/org/freedesktop/portal/desktop/request/{unique}/{token}");
    let obj_path = ObjectPath::try_from(req_path.as_str())
        .map_err(|e| format!("요청 객체 경로 생성 실패: {e}"))?;

    let req_proxy = Proxy::new(
        &conn,
        "org.freedesktop.portal.Desktop",
        obj_path,
        "org.freedesktop.portal.Request",
    )
    .await
    .map_err(|e| format!("Portal Request 프록시 생성 실패: {e}"))?;

    let mut response_stream = req_proxy
        .receive_signal("Response")
        .await
        .map_err(|e| format!("Portal Response 시그널 구독 실패: {e}"))?;

    let chooser = Proxy::new(
        &conn,
        "org.freedesktop.portal.Desktop",
        "/org/freedesktop/portal/desktop",
        "org.freedesktop.portal.FileChooser",
    )
    .await
    .map_err(|e| format!("FileChooser 프록시 생성 실패: {e}"))?;

    let mut options: HashMap<&str, Value<'_>> = HashMap::new();
    options.insert("handle_token", Value::from(token.as_str()));
    options.insert("multiple", Value::from(true));
    options.insert("accept_label", Value::from("첨부"));

    let _handle: zbus::zvariant::OwnedObjectPath = chooser
        .call("OpenFile", &("", "첨부할 파일 선택", options))
        .await
        .map_err(|e| format!("파일 선택기 호출 실패: {e}"))?;

    let Some(signal) = response_stream.next().await else {
        return Ok(Vec::new());
    };

    let (code, results): (u32, HashMap<String, OwnedValue>) = signal
        .body()
        .deserialize()
        .map_err(|e| format!("파일 선택기 응답 파싱 실패: {e}"))?;

    if code != 0 {
        // 사용자가 취소한 경우 빈 목록 반환
        return Ok(Vec::new());
    }

    let mut paths = Vec::new();
    if let Some(uris_val) = results.get("uris")
        && let Ok(uris) = <Vec<String>>::try_from(uris_val.clone())
    {
        for uri in uris {
            if let Some(path) = file_uri_to_path(&uri)
                && path.is_file()
            {
                paths.push(path);
            }
        }
    }
    Ok(paths)
}

/// XDG Desktop Portal을 통해 원격 제어 허용 폴더를 하나 선택한다.
pub async fn pick_folder() -> Result<Option<PathBuf>, String> {
    let conn = Connection::session()
        .await
        .map_err(|e| format!("세션 D-Bus 연결 실패: {e}"))?;

    let unique = conn
        .unique_name()
        .ok_or_else(|| "D-Bus 고유 이름을 가져올 수 없습니다".to_string())?
        .as_str()
        .trim_start_matches(':')
        .replace('.', "_");

    let seq = REQ_COUNTER.fetch_add(1, Ordering::Relaxed);
    let token = format!("gemini_ui_dir_{}_{seq}", std::process::id());
    let req_path = format!("/org/freedesktop/portal/desktop/request/{unique}/{token}");
    let obj_path = ObjectPath::try_from(req_path.as_str())
        .map_err(|e| format!("요청 객체 경로 생성 실패: {e}"))?;

    let req_proxy = Proxy::new(
        &conn,
        "org.freedesktop.portal.Desktop",
        obj_path,
        "org.freedesktop.portal.Request",
    )
    .await
    .map_err(|e| format!("Portal Request 프록시 생성 실패: {e}"))?;

    let mut response_stream = req_proxy
        .receive_signal("Response")
        .await
        .map_err(|e| format!("Portal Response 시그널 구독 실패: {e}"))?;

    let chooser = Proxy::new(
        &conn,
        "org.freedesktop.portal.Desktop",
        "/org/freedesktop/portal/desktop",
        "org.freedesktop.portal.FileChooser",
    )
    .await
    .map_err(|e| format!("FileChooser 프록시 생성 실패: {e}"))?;

    let mut options: HashMap<&str, Value<'_>> = HashMap::new();
    options.insert("handle_token", Value::from(token.as_str()));
    options.insert("directory", Value::from(true));
    options.insert("multiple", Value::from(false));
    options.insert("accept_label", Value::from("허용"));

    let _handle: zbus::zvariant::OwnedObjectPath = chooser
        .call("OpenFile", &("", "원격 제어에 허용할 폴더 선택", options))
        .await
        .map_err(|e| format!("폴더 선택기 호출 실패: {e}"))?;

    let Some(signal) = response_stream.next().await else {
        return Ok(None);
    };

    let (code, results): (u32, HashMap<String, OwnedValue>) = signal
        .body()
        .deserialize()
        .map_err(|e| format!("폴더 선택기 응답 파싱 실패: {e}"))?;

    if code != 0 {
        return Ok(None);
    }

    if let Some(uris_val) = results.get("uris")
        && let Ok(uris) = <Vec<String>>::try_from(uris_val.clone())
    {
        for uri in uris {
            if let Some(path) = file_uri_to_path(&uri)
                && path.is_dir()
            {
                return Ok(Some(path));
            }
        }
    }
    Ok(None)
}

fn file_uri_to_path(uri: &str) -> Option<PathBuf> {
    let raw = uri.strip_prefix("file://")?;
    let bytes = raw.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && i + 2 < bytes.len()
            && let (Some(hi), Some(lo)) = (hex_val(bytes[i + 1]), hex_val(bytes[i + 2]))
        {
            out.push((hi << 4) | lo);
            i += 3;
            continue;
        }
        out.push(bytes[i]);
        i += 1;
    }
    let decoded = String::from_utf8(out).ok()?;
    Some(PathBuf::from(decoded))
}

fn hex_val(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}
