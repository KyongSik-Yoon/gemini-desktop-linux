use crate::i18n::{Locale, pick};
use iced::futures::StreamExt;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::{mpsc, watch};
use zbus::object_server::SignalEmitter;
use zbus::zvariant::{OwnedObjectPath, OwnedValue, Value};
use zbus::{Connection, Proxy, connection, interface};

const SINGLE_INSTANCE_BUS_NAME: &str = "com.sam.GeminiUI";
const SINGLE_INSTANCE_OBJ_PATH: &str = "/com/sam/GeminiUI";
const SNI_OBJ_PATH: &str = "/StatusNotifierItem";
const MENU_OBJ_PATH: &str = "/MenuBar";
const WATCHER_BUS_NAME: &str = "org.kde.StatusNotifierWatcher";
const WATCHER_OBJ_PATH: &str = "/StatusNotifierWatcher";
const WATCHER_IFACE: &str = "org.kde.StatusNotifierWatcher";

// DBusMenu 항목 ID
const MENU_ID_ROOT: i32 = 0;
const MENU_ID_TOGGLE_WINDOW: i32 = 1;
const MENU_ID_NEW_CHAT: i32 = 2;
const MENU_ID_SEP_1: i32 = 3;
const MENU_ID_STATUS: i32 = 4;
const MENU_ID_TOGGLE_REMOTE: i32 = 5;
const MENU_ID_SETTINGS: i32 = 6;
const MENU_ID_SEP_2: i32 = 7;
const MENU_ID_QUIT: i32 = 8;

/// CLI 실행 모드.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LaunchMode {
    Normal,
    StartInTray,
    Toggle,
}

impl LaunchMode {
    pub fn from_args(args: impl IntoIterator<Item = String>) -> Result<Self, String> {
        let mut mode = Self::Normal;
        for arg in args.into_iter().skip(1) {
            match arg.as_str() {
                "--tray" | "--minimized" | "--background" => mode = Self::StartInTray,
                "--toggle" => mode = Self::Toggle,
                "-h" | "--help" => {
                    return Err(concat!(
                        "Usage: gemini-ui [OPTIONS]\n\n",
                        "Options:\n",
                        "  --tray, --minimized, --background   Start minimized in the system tray\n",
                        "  --toggle                            Toggle existing window or launch\n",
                        "  -h, --help                          Show this help message\n",
                    )
                    .to_string());
                }
                _ => {}
            }
        }
        Ok(mode)
    }
}

/// 이미 실행 중인 `gemini-ui` 인스턴스가 D-Bus에 등록되어 있으면 창을 활성화/토글하고 `true`를 반환한다.
pub fn try_activate_existing_instance(mode: LaunchMode) -> bool {
    let Ok(rt) = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    else {
        return false;
    };

    rt.block_on(async move {
        let res = tokio::time::timeout(Duration::from_millis(400), async {
            let conn = Connection::session().await.ok()?;
            let proxy = Proxy::new(
                &conn,
                SINGLE_INSTANCE_BUS_NAME,
                SINGLE_INSTANCE_OBJ_PATH,
                SINGLE_INSTANCE_BUS_NAME,
            )
            .await
            .ok()?;

            match mode {
                LaunchMode::Normal => {
                    let _: () = proxy.call("ShowWindow", &()).await.ok()?;
                }
                LaunchMode::Toggle => {
                    let _: () = proxy.call("ToggleWindow", &()).await.ok()?;
                }
                LaunchMode::StartInTray => {
                    let _: bool = proxy.call("Ping", &()).await.ok()?;
                }
            }
            Some(())
        })
        .await;
        matches!(res, Ok(Some(())))
    })
}

/// 트레이 아이콘 및 메뉴에 표시할 실시간 앱 상태 스냅샷.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TraySnapshot {
    pub window_visible: bool,
    pub locale: Locale,
    pub daemon_connected: bool,
    pub remote_enabled: bool,
    pub punctual_connected: bool,
    pub current_tool: String,
    pub is_streaming: bool,
}

impl Default for TraySnapshot {
    fn default() -> Self {
        Self {
            window_visible: true,
            locale: Locale::from_env(),
            daemon_connected: false,
            remote_enabled: false,
            punctual_connected: false,
            current_tool: String::new(),
            is_streaming: false,
        }
    }
}

impl TraySnapshot {
    fn status_line(&self) -> String {
        if !self.daemon_connected {
            return pick(self.locale, "데몬 미연결", "Daemon disconnected").to_string();
        }
        if !self.current_tool.is_empty() {
            return if self.locale.is_en() {
                format!("Running tool: {}", self.current_tool)
            } else {
                format!("도구 실행 중: {}", self.current_tool)
            };
        }
        if self.remote_enabled {
            if self.punctual_connected {
                pick(self.locale, "원격 제어: 연결됨", "Remote control: Connected").to_string()
            } else {
                pick(self.locale, "원격 제어: 대기 중", "Remote control: Standby").to_string()
            }
        } else {
            pick(self.locale, "원격 제어: 꺼짐", "Remote control: Off").to_string()
        }
    }

    fn tooltip_description(&self) -> String {
        let base = self.status_line();
        if self.is_streaming {
            if self.locale.is_en() {
                format!("{base} · Responding...")
            } else {
                format!("{base} · 응답 생성 중...")
            }
        } else {
            base
        }
    }
}

/// 트레이에서 메인 앱으로 전달되는 사용자 액션.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TrayAction {
    ShowWindow,
    ToggleWindow,
    NewChat,
    ToggleRemote,
    OpenSettings,
    Quit,
}

/// 트레이 서비스에서 `App::update`로 보내는 구독 이벤트.
#[derive(Debug, Clone)]
pub enum TrayEvent {
    Ready {
        available: bool,
        updater: Option<TrayUpdateSender>,
    },
    WatcherAvailabilityChanged(bool),
    Action(TrayAction),
}

/// `App`에서 트레이 상태를 비동기로 갱신하기 위한 경량 송신 핸들.
#[derive(Debug, Clone)]
pub struct TrayUpdateSender {
    tx: watch::Sender<TraySnapshot>,
}

impl TrayUpdateSender {
    /// 실제 값이 바뀐 경우에만 스냅샷을 갱신해 불필요한 할당과 D-Bus 시그널을 방지한다.
    #[allow(clippy::too_many_arguments)]
    pub fn sync(
        &self,
        window_visible: bool,
        locale: Locale,
        daemon_connected: bool,
        remote_enabled: bool,
        punctual_connected: bool,
        current_tool: &str,
        is_streaming: bool,
    ) {
        self.tx.send_if_modified(|cur| {
            if cur.window_visible == window_visible
                && cur.locale == locale
                && cur.daemon_connected == daemon_connected
                && cur.remote_enabled == remote_enabled
                && cur.punctual_connected == punctual_connected
                && cur.current_tool == current_tool
                && cur.is_streaming == is_streaming
            {
                return false;
            }
            cur.window_visible = window_visible;
            cur.locale = locale;
            cur.daemon_connected = daemon_connected;
            cur.remote_enabled = remote_enabled;
            cur.punctual_connected = punctual_connected;
            if cur.current_tool != current_tool {
                cur.current_tool.clear();
                cur.current_tool.push_str(current_tool);
            }
            cur.is_streaming = is_streaming;
            true
        });
    }
}

type PixmapEntry = (i32, i32, Vec<u8>);
type ToolTipTuple = (String, Vec<PixmapEntry>, String, String);
type MenuLayoutNode = (i32, HashMap<String, OwnedValue>, Vec<OwnedValue>);

struct SharedTrayState {
    snapshot: TraySnapshot,
    revision: u32,
}

struct StatusNotifierItem {
    state: Arc<Mutex<SharedTrayState>>,
    action_tx: mpsc::UnboundedSender<TrayAction>,
    icon_pixmap: Vec<PixmapEntry>,
    icon_theme_path: String,
}

#[interface(name = "org.kde.StatusNotifierItem")]
impl StatusNotifierItem {
    #[zbus(property)]
    async fn category(&self) -> &'static str {
        "ApplicationStatus"
    }

    #[zbus(property)]
    async fn id(&self) -> &'static str {
        "gemini-ui"
    }

    #[zbus(property)]
    async fn title(&self) -> &'static str {
        "Gemini"
    }

    #[zbus(property)]
    async fn status(&self) -> &'static str {
        "Active"
    }

    #[zbus(property)]
    async fn window_id(&self) -> i32 {
        0
    }

    #[zbus(property)]
    async fn icon_name(&self) -> &'static str {
        "gemini-ui"
    }

    #[zbus(property)]
    async fn icon_pixmap(&self) -> Vec<PixmapEntry> {
        self.icon_pixmap.clone()
    }

    #[zbus(property)]
    async fn icon_theme_path(&self) -> String {
        self.icon_theme_path.clone()
    }

    #[zbus(property)]
    async fn overlay_icon_name(&self) -> &'static str {
        ""
    }

    #[zbus(property)]
    async fn overlay_icon_pixmap(&self) -> Vec<PixmapEntry> {
        Vec::new()
    }

    #[zbus(property)]
    async fn attention_icon_name(&self) -> &'static str {
        ""
    }

    #[zbus(property)]
    async fn attention_icon_pixmap(&self) -> Vec<PixmapEntry> {
        Vec::new()
    }

    #[zbus(property)]
    async fn attention_movie_name(&self) -> &'static str {
        ""
    }

    #[zbus(property)]
    async fn tool_tip(&self) -> ToolTipTuple {
        let desc = self
            .state
            .lock()
            .map(|g| g.snapshot.tooltip_description())
            .unwrap_or_default();
        ("gemini-ui".into(), Vec::new(), "Gemini".into(), desc)
    }

    #[zbus(property)]
    async fn item_is_menu(&self) -> bool {
        false
    }

    #[zbus(property)]
    async fn menu(&self) -> OwnedObjectPath {
        OwnedObjectPath::try_from(MENU_OBJ_PATH).expect("static dbusmenu object path")
    }

    async fn activate(&self, _x: i32, _y: i32) {
        let _ = self.action_tx.send(TrayAction::ToggleWindow);
    }

    async fn secondary_activate(&self, _x: i32, _y: i32) {
        let _ = self.action_tx.send(TrayAction::NewChat);
    }

    async fn context_menu(&self, _x: i32, _y: i32) {
        // 표준 DBusMenu(/MenuBar)로 컨텍스트 메뉴를 제공한다.
    }

    async fn scroll(&self, _delta: i32, _orientation: &str) {}

    #[zbus(signal)]
    async fn new_title(emitter: &SignalEmitter<'_>) -> zbus::Result<()>;

    #[zbus(signal)]
    async fn new_icon(emitter: &SignalEmitter<'_>) -> zbus::Result<()>;

    #[zbus(signal)]
    async fn new_attention_icon(emitter: &SignalEmitter<'_>) -> zbus::Result<()>;

    #[zbus(signal)]
    async fn new_overlay_icon(emitter: &SignalEmitter<'_>) -> zbus::Result<()>;

    #[zbus(signal)]
    async fn new_tool_tip(emitter: &SignalEmitter<'_>) -> zbus::Result<()>;

    #[zbus(signal)]
    async fn new_status(emitter: &SignalEmitter<'_>, status: &str) -> zbus::Result<()>;
}

struct DBusMenu {
    state: Arc<Mutex<SharedTrayState>>,
    action_tx: mpsc::UnboundedSender<TrayAction>,
    icon_theme_paths: Vec<String>,
}

impl DBusMenu {
    fn item_props(id: i32, snap: &TraySnapshot) -> Option<HashMap<String, OwnedValue>> {
        let mut props = HashMap::new();
        match id {
            MENU_ID_ROOT => {
                insert_str(&mut props, "children-display", "submenu");
            }
            MENU_ID_TOGGLE_WINDOW => {
                let label = if snap.window_visible {
                    pick(snap.locale, "Gemini 숨기기", "Hide Gemini")
                } else {
                    pick(snap.locale, "Gemini 열기", "Show Gemini")
                };
                insert_str(&mut props, "label", label);
                props.insert("enabled".into(), OwnedValue::from(true));
                props.insert("visible".into(), OwnedValue::from(true));
            }
            MENU_ID_NEW_CHAT => {
                insert_str(
                    &mut props,
                    "label",
                    pick(snap.locale, "새 대화", "New Chat"),
                );
                props.insert("enabled".into(), OwnedValue::from(true));
                props.insert("visible".into(), OwnedValue::from(true));
            }
            MENU_ID_SEP_1 | MENU_ID_SEP_2 => {
                insert_str(&mut props, "type", "separator");
                props.insert("visible".into(), OwnedValue::from(true));
            }
            MENU_ID_STATUS => {
                insert_str(&mut props, "label", &snap.status_line());
                props.insert("enabled".into(), OwnedValue::from(false));
                props.insert("visible".into(), OwnedValue::from(true));
            }
            MENU_ID_TOGGLE_REMOTE => {
                insert_str(
                    &mut props,
                    "label",
                    pick(snap.locale, "원격 제어", "Remote Control"),
                );
                insert_str(&mut props, "toggle-type", "checkmark");
                props.insert(
                    "toggle-state".into(),
                    OwnedValue::from(if snap.remote_enabled { 1i32 } else { 0i32 }),
                );
                props.insert("enabled".into(), OwnedValue::from(snap.daemon_connected));
                props.insert("visible".into(), OwnedValue::from(true));
            }
            MENU_ID_SETTINGS => {
                insert_str(
                    &mut props,
                    "label",
                    pick(snap.locale, "설정", "Settings"),
                );
                props.insert("enabled".into(), OwnedValue::from(true));
                props.insert("visible".into(), OwnedValue::from(true));
            }
            MENU_ID_QUIT => {
                insert_str(&mut props, "label", pick(snap.locale, "종료", "Quit"));
                props.insert("enabled".into(), OwnedValue::from(true));
                props.insert("visible".into(), OwnedValue::from(true));
            }
            _ => return None,
        }
        Some(props)
    }

    fn filter_props(
        mut props: HashMap<String, OwnedValue>,
        property_names: &[String],
    ) -> HashMap<String, OwnedValue> {
        if property_names.is_empty() {
            return props;
        }
        props.retain(|k, _| property_names.iter().any(|wanted| wanted == k));
        props
    }

    fn build_layout(
        snap: &TraySnapshot,
        parent_id: i32,
        recursion_depth: i32,
        property_names: &[String],
    ) -> MenuLayoutNode {
        let root_props = Self::item_props(parent_id, snap)
            .map(|p| Self::filter_props(p, property_names))
            .unwrap_or_default();

        if parent_id != MENU_ID_ROOT || recursion_depth == 0 {
            return (parent_id, root_props, Vec::new());
        }

        let child_ids = [
            MENU_ID_TOGGLE_WINDOW,
            MENU_ID_NEW_CHAT,
            MENU_ID_SEP_1,
            MENU_ID_STATUS,
            MENU_ID_TOGGLE_REMOTE,
            MENU_ID_SETTINGS,
            MENU_ID_SEP_2,
            MENU_ID_QUIT,
        ];

        let mut children = Vec::with_capacity(child_ids.len());
        for cid in child_ids {
            if let Some(raw_props) = Self::item_props(cid, snap) {
                let filtered = Self::filter_props(raw_props, property_names);
                if let Ok(val) = OwnedValue::try_from(Value::from((cid, filtered, Vec::<OwnedValue>::new())))
                {
                    children.push(val);
                }
            }
        }
        (MENU_ID_ROOT, root_props, children)
    }
}

#[interface(name = "com.canonical.dbusmenu")]
impl DBusMenu {
    #[zbus(property)]
    async fn version(&self) -> u32 {
        3
    }

    #[zbus(property)]
    async fn text_direction(&self) -> &'static str {
        "ltr"
    }

    #[zbus(property)]
    async fn status(&self) -> &'static str {
        "normal"
    }

    #[zbus(property)]
    async fn icon_theme_path(&self) -> Vec<String> {
        self.icon_theme_paths.clone()
    }

    async fn get_layout(
        &self,
        parent_id: i32,
        recursion_depth: i32,
        property_names: Vec<String>,
    ) -> (u32, MenuLayoutNode) {
        let (revision, snap) = self
            .state
            .lock()
            .map(|g| (g.revision, g.snapshot.clone()))
            .unwrap_or_else(|_| (1, TraySnapshot::default()));
        let layout = Self::build_layout(&snap, parent_id, recursion_depth, &property_names);
        (revision, layout)
    }

    async fn get_group_properties(
        &self,
        ids: Vec<i32>,
        property_names: Vec<String>,
    ) -> Vec<(i32, HashMap<String, OwnedValue>)> {
        let snap = self
            .state
            .lock()
            .map(|g| g.snapshot.clone())
            .unwrap_or_default();
        let mut out = Vec::new();
        for id in ids {
            if let Some(props) = Self::item_props(id, &snap) {
                out.push((id, Self::filter_props(props, &property_names)));
            }
        }
        out
    }

    async fn get_property(&self, id: i32, name: String) -> zbus::fdo::Result<OwnedValue> {
        let snap = self
            .state
            .lock()
            .map(|g| g.snapshot.clone())
            .unwrap_or_default();
        Self::item_props(id, &snap)
            .and_then(|mut props| props.remove(&name))
            .ok_or_else(|| zbus::fdo::Error::Failed(format!("property not found: {id}.{name}")))
    }

    async fn event(&self, id: i32, event_id: String, _data: OwnedValue, _timestamp: u32) {
        if event_id != "clicked" {
            return;
        }
        let action = match id {
            MENU_ID_TOGGLE_WINDOW => Some(TrayAction::ToggleWindow),
            MENU_ID_NEW_CHAT => Some(TrayAction::NewChat),
            MENU_ID_TOGGLE_REMOTE => Some(TrayAction::ToggleRemote),
            MENU_ID_SETTINGS => Some(TrayAction::OpenSettings),
            MENU_ID_QUIT => Some(TrayAction::Quit),
            _ => None,
        };
        if let Some(act) = action {
            let _ = self.action_tx.send(act);
        }
    }

    async fn event_group(&self, events: Vec<(i32, String, OwnedValue, u32)>) -> Vec<i32> {
        for (id, event_id, data, timestamp) in events {
            self.event(id, event_id, data, timestamp).await;
        }
        Vec::new()
    }

    async fn about_to_show(&self, _id: i32) -> bool {
        false
    }

    async fn about_to_show_group(&self, _ids: Vec<i32>) -> (Vec<i32>, Vec<i32>) {
        (Vec::new(), Vec::new())
    }

    #[zbus(signal)]
    async fn layout_updated(
        emitter: &SignalEmitter<'_>,
        revision: u32,
        parent: i32,
    ) -> zbus::Result<()>;

    #[zbus(signal)]
    async fn items_properties_updated(
        emitter: &SignalEmitter<'_>,
        updated_props: Vec<(i32, HashMap<String, OwnedValue>)>,
        removed_props: Vec<(i32, Vec<String>)>,
    ) -> zbus::Result<()>;
}

/// 단일 인스턴스 제어용 D-Bus 인터페이스 (`com.sam.GeminiUI`).
struct SingleInstanceBridge {
    action_tx: mpsc::UnboundedSender<TrayAction>,
}

#[interface(name = "com.sam.GeminiUI")]
impl SingleInstanceBridge {
    async fn show_window(&self) {
        let _ = self.action_tx.send(TrayAction::ShowWindow);
    }

    async fn toggle_window(&self) {
        let _ = self.action_tx.send(TrayAction::ToggleWindow);
    }

    async fn new_chat(&self) {
        let _ = self.action_tx.send(TrayAction::NewChat);
    }

    async fn ping(&self) -> bool {
        true
    }
}

fn insert_str(map: &mut HashMap<String, OwnedValue>, key: &str, val: &str) {
    if let Ok(ov) = OwnedValue::try_from(Value::from(val)) {
        map.insert(key.to_string(), ov);
    }
}

/// 사용자 로컬 아이콘 테마 경로(`~/.local/share/icons`)를 반환한다.
fn resolve_icon_theme_path() -> String {
    let base = std::env::var("XDG_DATA_HOME")
        .ok()
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var("HOME")
                .ok()
                .map(|h| PathBuf::from(h).join(".local").join("share"))
        });
    if let Some(dir) = base {
        let icons = dir.join("icons");
        if icons.is_dir() {
            return icons.to_string_lossy().to_string();
        }
    }
    String::new()
}

/// SNI 표준(네트워크 바이트 순서 big-endian ARGB32)으로 4색 Gemini 스파클 트레이 아이콘을 생성한다.
/// 아이콘 테마 캐시가 없거나 개발 실행 시에도 선명하게 표시되는 인메모리 폴백 픽스맵이다.
fn render_gemini_tray_icon(size: i32) -> PixmapEntry {
    let w = size.max(16) as usize;
    let h = w;
    let mut bytes = Vec::with_capacity(w * h * 4);
    let half = (w as f32) * 0.5;
    // 2x2 슈퍼샘플링으로 가장자리 계단 현상을 억제한다.
    let offsets = [(-0.25f32, -0.25f32), (0.25, -0.25), (-0.25, 0.25), (0.25, 0.25)];

    for py in 0..h {
        for px in 0..w {
            let mut acc_a = 0.0f32;
            let mut acc_r = 0.0f32;
            let mut acc_g = 0.0f32;
            let mut acc_b = 0.0f32;

            for (ox, oy) in offsets {
                let nx = ((px as f32 + 0.5 + ox) - half) / half;
                let ny = ((py as f32 + 0.5 + oy) - half) / half;
                // 4각 아스트로이드(|x|^0.62 + |y|^0.62 <= 0.88^0.62)로 Gemini 스파클 실루엣 구성
                let d = nx.abs().powf(0.62) + ny.abs().powf(0.62);
                let threshold = 0.88f32.powf(0.62);
                if d <= threshold {
                    let edge = ((threshold - d) * 14.0).clamp(0.0, 1.0);
                    // 대각선(y = x, y = -x) 기준 4개 꽃잎 색상 (상: 빨강, 우: 파랑, 하: 초록, 좌: 노랑)
                    let (r, g, b) = if ny < -nx.abs() * 0.92 {
                        (234.0, 67.0, 53.0) // #EA4335 (Top)
                    } else if nx > ny.abs() * 0.92 {
                        (49.0, 134.0, 255.0) // #3186FF (Right)
                    } else if ny > nx.abs() * 0.92 {
                        (0.0, 185.0, 92.0) // #00B95C (Bottom)
                    } else if nx < -ny.abs() * 0.92 {
                        (251.0, 188.0, 4.0) // #FBBC04 (Left)
                    } else {
                        (66.0, 133.0, 244.0) // #4285F4 (Center blend)
                    };
                    acc_a += edge;
                    acc_r += r * edge;
                    acc_g += g * edge;
                    acc_b += b * edge;
                }
            }

            let alpha = (acc_a / 4.0).clamp(0.0, 1.0);
            if alpha <= 0.001 {
                bytes.extend_from_slice(&[0, 0, 0, 0]);
            } else {
                let inv = 1.0 / acc_a;
                let a = (alpha * 255.0).round().clamp(0.0, 255.0) as u8;
                let r = (acc_r * inv).round().clamp(0.0, 255.0) as u8;
                let g = (acc_g * inv).round().clamp(0.0, 255.0) as u8;
                let b = (acc_b * inv).round().clamp(0.0, 255.0) as u8;
                bytes.extend_from_slice(&[a, r, g, b]);
            }
        }
    }

    (w as i32, h as i32, bytes)
}

async fn register_with_watcher(conn: &Connection, service_name: &str) -> bool {
    let Ok(proxy) = Proxy::new(conn, WATCHER_BUS_NAME, WATCHER_OBJ_PATH, WATCHER_IFACE).await
    else {
        return false;
    };

    let registered = if proxy
        .call::<_, _, ()>("RegisterStatusNotifierItem", &(service_name,))
        .await
        .is_ok()
    {
        true
    } else {
        proxy
            .call::<_, _, ()>("RegisterStatusNotifierItem", &(SNI_OBJ_PATH,))
            .await
            .is_ok()
    };

    if !registered {
        return false;
    }

    // 호스트가 실제로 붙어 있는지 확인하되, 프로퍼티 조회 실패 시에는 등록 성공을 신뢰한다.
    proxy
        .get_property::<bool>("IsStatusNotifierHostRegistered")
        .await
        .unwrap_or(true)
}

/// D-Bus SNI + DBusMenu + 단일 인스턴스 활성화 서비스를 백그라운드 스트림으로 실행한다.
/// 유휴 상태에서는 D-Bus 소켓 및 watch 채널 이벤트에만 깨어나므로 폴링 오버헤드가 0이다.
pub fn run_tray_service() -> impl iced::futures::Stream<Item = TrayEvent> {
    iced::stream::channel(
        32,
        |mut out: iced::futures::channel::mpsc::Sender<TrayEvent>| async move {
            let shared = Arc::new(Mutex::new(SharedTrayState {
                snapshot: TraySnapshot::default(),
                revision: 1,
            }));
            let (action_tx, mut action_rx) = mpsc::unbounded_channel::<TrayAction>();
            let (snap_tx, mut snap_rx) = watch::channel(TraySnapshot::default());

            let icon_theme_path = resolve_icon_theme_path();
            let icon_theme_paths = if icon_theme_path.is_empty() {
                Vec::new()
            } else {
                vec![icon_theme_path.clone()]
            };

            let sni = StatusNotifierItem {
                state: Arc::clone(&shared),
                action_tx: action_tx.clone(),
                icon_pixmap: vec![render_gemini_tray_icon(32)],
                icon_theme_path,
            };
            let menu = DBusMenu {
                state: Arc::clone(&shared),
                action_tx: action_tx.clone(),
                icon_theme_paths,
            };
            let bridge = SingleInstanceBridge { action_tx };

            let sni_bus_name = format!("org.kde.StatusNotifierItem-{}-1", std::process::id());
            let Ok(builder) = connection::Builder::session() else {
                let _ = out.try_send(TrayEvent::Ready {
                    available: false,
                    updater: None,
                });
                std::future::pending::<()>().await;
                return;
            };

            let builder = match builder
                .serve_at(SNI_OBJ_PATH, sni)
                .and_then(|b| b.serve_at(MENU_OBJ_PATH, menu))
                .and_then(|b| b.serve_at(SINGLE_INSTANCE_OBJ_PATH, bridge))
            {
                Ok(b) => b,
                Err(_) => {
                    let _ = out.try_send(TrayEvent::Ready {
                        available: false,
                        updater: None,
                    });
                    std::future::pending::<()>().await;
                    return;
                }
            };

            let Ok(conn) = builder.build().await else {
                let _ = out.try_send(TrayEvent::Ready {
                    available: false,
                    updater: None,
                });
                std::future::pending::<()>().await;
                return;
            };

            // 단일 인스턴스 이름 및 SNI 서비스 이름 요청 (이미 점유되었더라도 계속 진행)
            let _ = conn.request_name(SINGLE_INSTANCE_BUS_NAME).await;
            let _ = conn.request_name(sni_bus_name.as_str()).await;

            let initial_available = register_with_watcher(&conn, &sni_bus_name).await;
            let _ = out.try_send(TrayEvent::Ready {
                available: initial_available,
                updater: Some(TrayUpdateSender { tx: snap_tx }),
            });

            // 스냅샷 변경 시 D-Bus 툴팁 및 메뉴 레이아웃 시그널을 발행한다.
            let snap_conn = conn.clone();
            let snap_shared = Arc::clone(&shared);
            let snap_task = tokio::spawn(async move {
                while snap_rx.changed().await.is_ok() {
                    let next_snap = snap_rx.borrow().clone();
                    let rev = if let Ok(mut g) = snap_shared.lock() {
                        g.snapshot = next_snap;
                        g.revision = g.revision.saturating_add(1);
                        g.revision
                    } else {
                        continue;
                    };
                    if let Ok(sni_ref) = snap_conn
                        .object_server()
                        .interface::<_, StatusNotifierItem>(SNI_OBJ_PATH)
                        .await
                    {
                        let _ = StatusNotifierItem::new_tool_tip(sni_ref.signal_emitter()).await;
                    }
                    if let Ok(menu_ref) = snap_conn
                        .object_server()
                        .interface::<_, DBusMenu>(MENU_OBJ_PATH)
                        .await
                    {
                        let _ =
                            DBusMenu::layout_updated(menu_ref.signal_emitter(), rev, MENU_ID_ROOT)
                                .await;
                    }
                }
            });

            // StatusNotifierWatcher 재시작(Quickshell/Waybar 리로드 등)을 감지해 자동 재등록한다.
            let watcher_conn = conn.clone();
            let watcher_out = out.clone();
            let watcher_task = tokio::spawn(async move {
                let Ok(dbus_proxy) = zbus::fdo::DBusProxy::new(&watcher_conn).await else {
                    return;
                };
                let Ok(mut owner_changes) = dbus_proxy.receive_name_owner_changed().await else {
                    return;
                };
                let mut watcher_out = watcher_out;
                while let Some(sig) = owner_changes.next().await {
                    if let Ok(args) = sig.args()
                        && args.name().as_str() == WATCHER_BUS_NAME
                    {
                        let available = if args.new_owner().is_some() {
                            register_with_watcher(&watcher_conn, &sni_bus_name).await
                        } else {
                            false
                        };
                        if let Err(err) =
                            watcher_out.try_send(TrayEvent::WatcherAvailabilityChanged(available))
                            && err.is_disconnected()
                        {
                            break;
                        }
                    }
                }
            });

            while let Some(action) = action_rx.recv().await {
                if let Err(err) = out.try_send(TrayEvent::Action(action))
                    && err.is_disconnected()
                {
                    break;
                }
            }

            snap_task.abort();
            watcher_task.abort();
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cli_인자_파싱_트레이_및_토글_모드() {
        assert_eq!(
            LaunchMode::from_args(["gemini-ui".into()]).unwrap(),
            LaunchMode::Normal
        );
        assert_eq!(
            LaunchMode::from_args(["gemini-ui".into(), "--tray".into()]).unwrap(),
            LaunchMode::StartInTray
        );
        assert_eq!(
            LaunchMode::from_args(["gemini-ui".into(), "--minimized".into()]).unwrap(),
            LaunchMode::StartInTray
        );
        assert_eq!(
            LaunchMode::from_args(["gemini-ui".into(), "--toggle".into()]).unwrap(),
            LaunchMode::Toggle
        );
        assert!(LaunchMode::from_args(["gemini-ui".into(), "--help".into()]).is_err());
    }

    #[test]
    fn 트레이_아이콘_픽스맵_크기_및_비투명_픽셀_검증() {
        let (w, h, bytes) = render_gemini_tray_icon(32);
        assert_eq!((w, h), (32, 32));
        assert_eq!(bytes.len(), 32 * 32 * 4);
        // 중앙 픽셀(16, 16)은 스파클 내부이므로 불투명해야 한다.
        let center_idx = (16 * 32 + 16) * 4;
        assert!(bytes[center_idx] > 200, "center alpha={}", bytes[center_idx]);
        // 모서리 픽셀(0, 0)은 스파클 외부이므로 투명해야 한다.
        assert_eq!(bytes[0], 0);
    }

    #[test]
    fn dbusmenu_레이아웃_로케일_및_상태_반영() {
        let snap_ko = TraySnapshot {
            window_visible: true,
            locale: Locale::Ko,
            daemon_connected: true,
            remote_enabled: true,
            punctual_connected: true,
            current_tool: String::new(),
            is_streaming: false,
        };
        let (root_id, _, children) = DBusMenu::build_layout(&snap_ko, MENU_ID_ROOT, -1, &[]);
        assert_eq!(root_id, MENU_ID_ROOT);
        assert_eq!(children.len(), 8);
        assert_eq!(snap_ko.status_line(), "원격 제어: 연결됨");

        let toggle_props = DBusMenu::item_props(MENU_ID_TOGGLE_WINDOW, &snap_ko).unwrap();
        let label = String::try_from(toggle_props.get("label").unwrap().clone()).unwrap();
        assert_eq!(label, "Gemini 숨기기");

        let snap_en_hidden = TraySnapshot {
            window_visible: false,
            locale: Locale::En,
            daemon_connected: false,
            remote_enabled: false,
            punctual_connected: false,
            current_tool: String::new(),
            is_streaming: true,
        };
        let toggle_props_en = DBusMenu::item_props(MENU_ID_TOGGLE_WINDOW, &snap_en_hidden).unwrap();
        let label_en = String::try_from(toggle_props_en.get("label").unwrap().clone()).unwrap();
        assert_eq!(label_en, "Show Gemini");
        assert!(snap_en_hidden.tooltip_description().contains("Responding..."));
    }
}
