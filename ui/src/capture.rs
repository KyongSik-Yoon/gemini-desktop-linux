use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::process::Command as AsyncCommand;

fn timestamp_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0)
}

/// 화면 영역 또는 전체 화면을 캡처해 PNG 임시 파일 경로를 반환한다.
/// Wayland에서는 `slurp + grim`(폴백: `grim`)을, X11에서는 ImageMagick `import` 또는 `scrot`를 사용한다.
pub async fn capture_screen() -> Result<PathBuf, String> {
    let out_path = PathBuf::from(format!("/tmp/gemini-screen-{}.png", timestamp_ms()));
    let is_wayland = std::env::var_os("WAYLAND_DISPLAY").is_some();

    if is_wayland {
        // 1. Wayland: slurp로 영역 선택 시도
        match AsyncCommand::new("slurp").output().await {
            Ok(sel) => {
                if !sel.status.success() {
                    return Err("화면 캡처가 취소되었습니다".into());
                }
                let geom = String::from_utf8_lossy(&sel.stdout).trim().to_string();
                if geom.is_empty() {
                    return Err("선택된 화면 영역이 없습니다".into());
                }
                let cap = AsyncCommand::new("grim")
                    .args(["-g", &geom, &out_path.to_string_lossy()])
                    .output()
                    .await
                    .map_err(|e| format!("grim 실행 실패: {e}"))?;
                if !cap.status.success() {
                    let err = String::from_utf8_lossy(&cap.stderr);
                    return Err(format!("화면 캡처 실패: {}", err.trim()));
                }
                return Ok(out_path);
            }
            Err(_) => {
                // slurp 미설치 시 grim 전체 화면 폴백
                if let Ok(cap) = AsyncCommand::new("grim").arg(&out_path).output().await {
                    if cap.status.success() {
                        return Ok(out_path);
                    }
                    let err = String::from_utf8_lossy(&cap.stderr);
                    return Err(format!("화면 캡처 실패: {}", err.trim()));
                }
            }
        }
    }

    // 2. X11 폴백 (또는 Wayland에서 grim 미설치 시): ImageMagick import → scrot 순으로 시도
    if let Ok(cap) = AsyncCommand::new("import")
        .args(["-window", "root", &out_path.to_string_lossy()])
        .output()
        .await
    {
        if cap.status.success() {
            return Ok(out_path);
        }
    }

    if let Ok(cap) = AsyncCommand::new("scrot").arg(&out_path).output().await {
        if cap.status.success() {
            return Ok(out_path);
        }
        let err = String::from_utf8_lossy(&cap.stderr);
        return Err(format!("화면 캡처 실패: {}", err.trim()));
    }

    Err(if is_wayland {
        "화면 캡처 도구(grim/slurp)를 찾을 수 없습니다".into()
    } else {
        "화면 캡처 도구(import 또는 scrot)를 찾을 수 없습니다".into()
    })
}

/// PipeWire(`pw-record`) 또는 ALSA(`arecord`)를 이용한 실시간 음성 녹음 세션.
pub struct VoiceRecorder {
    child: Child,
    path: PathBuf,
}

impl VoiceRecorder {
    pub fn start() -> Result<Self, String> {
        let path = PathBuf::from(format!("/tmp/gemini-voice-{}.wav", timestamp_ms()));
        let path_str = path.to_string_lossy().to_string();

        // pw-record 우선, 실패 시 arecord 폴백
        let child = Command::new("pw-record")
            .args(["--format", "s16", "--rate", "16000", "--channels", "1", &path_str])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .or_else(|_| {
                Command::new("arecord")
                    .args(["-f", "S16_LE", "-r", "16000", "-c", "1", "-t", "wav", &path_str])
                    .stdin(Stdio::null())
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .spawn()
            })
            .map_err(|e| format!("마이크 녹음 시작 실패 (pw-record/arecord 확인 필요): {e}"))?;

        Ok(Self { child, path })
    }

    /// 녹음 프로세스에 SIGINT를 보내 WAV 헤더를 정상 마무리하고 파일 경로를 반환한다.
    pub fn stop(mut self) -> Result<PathBuf, String> {
        let pid = self.child.id().to_string();
        let _ = Command::new("kill")
            .args(["-INT", &pid])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();

        for _ in 0..20 {
            if let Ok(Some(_)) = self.child.try_wait() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(25));
        }
        let _ = self.child.kill();
        let _ = self.child.wait();

        let meta = std::fs::metadata(&self.path)
            .map_err(|e| format!("녹음 파일을 읽을 수 없습니다: {e}"))?;
        if meta.len() <= 44 {
            let _ = std::fs::remove_file(&self.path);
            return Err("녹음된 음성이 너무 짧습니다".into());
        }
        Ok(self.path.clone())
    }
}

impl Drop for VoiceRecorder {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
