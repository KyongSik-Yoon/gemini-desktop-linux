mod capture;
mod filepicker;
mod glow;
mod i18n;
mod icons;
mod ipc;
mod markdown_view;
mod pages;
mod popovers;
mod theme;
mod tray;

use capture::VoiceRecorder;
use glow::{AvatarCircleMask, WelcomeGlow};
use i18n::{LanguagePref, Locale, pick, t};
use iced::widget::{
    button, canvas, column, container, image, markdown, mouse_area, operation, row,
    scrollable, space, stack, text, text_editor, text_input,
};
use iced::{
    Alignment, Background, Border, Color, ContentFit, Element, Event, Fill, Length, Padding, Shadow,
    Size, Subscription, Task, Theme, Vector, clipboard, event, keyboard, mouse, system, task, time,
    window,
};
use icons::{Icon, sparkle_color};
use ipc::{
    BardModeOption, BotSummary, BotsResponse, ChatStreamChunk, ChatStreamRequest,
    ConversationSummary, ConversationTurn, DevicesResponse, GeneratedImage, HealthResponse,
    LibraryEntry, LibraryResponse, ListConversationsResponse, ListTurnsResponse, ModelsResponse,
    Notebook, NotebooksResponse, RecommendedSchedule, RegisteredDevice, RemoteEvent,
    RemoteStatusSnapshot, RemoteToolActivity, ScheduleRunResult, ScheduledTask, SchedulesResponse,
    SearchHit, SearchResponse, ShareResponse, StreamEvent, TurnArtifact, TurnAttachment,
    UIPreferences, UsageInfo, UserProfile,
};
use std::collections::{HashMap, HashSet, VecDeque};
use std::path::PathBuf;
use std::time::{Duration, Instant};
use theme::{AppTheme, ThemeMode, font_medium, font_regular, font_semibold};

const CHAT_SCROLL_ID: &str = "chat_scroll";
const SEARCH_INPUT_ID: &str = "search_modal_input";
pub(crate) const WELCOME_MAX_WIDTH: f32 = 660.0;
pub(crate) const CONTENT_MAX_WIDTH: f32 = 760.0;
pub(crate) const SIDEBAR_DEFAULT_WIDTH: f32 = 284.0;
pub(crate) const SIDEBAR_MIN_WIDTH: f32 = 220.0;
pub(crate) const SIDEBAR_MAX_WIDTH: f32 = 480.0;
const SIDEBAR_MAIN_MIN_RESERVE: f32 = 360.0;

// 기능 모드 코드 (서버 채팅 옵션 연동)
pub(crate) const FEATURE_MODE_DEEP_RESEARCH: i32 = 1;
pub(crate) const FEATURE_MODE_CANVAS: i32 = 2;
pub(crate) const FEATURE_MODE_FLASHCARDS: i32 = 9;
pub(crate) const FEATURE_MODE_QUIZ: i32 = 10;
pub(crate) const FEATURE_MODE_VIDEO: i32 = 11;
pub(crate) const FEATURE_MODE_IMAGE: i32 = 14;
pub(crate) const FEATURE_MODE_MUSIC: i32 = 21;
pub(crate) const FEATURE_MODE_GUIDED_LEARNING: i32 = 24;

// 사고 수준 코드
pub(crate) const THINKING_LOW: i32 = 1;
pub(crate) const THINKING_HIGH: i32 = 2;
pub(crate) const THINKING_MEDIUM: i32 = 4;

pub(crate) fn feature_mode_label(mode: i32) -> Option<&'static str> {
    match mode {
        FEATURE_MODE_DEEP_RESEARCH => Some("Deep Research"),
        FEATURE_MODE_CANVAS => Some("Canvas"),
        FEATURE_MODE_FLASHCARDS => Some("플래시카드"),
        FEATURE_MODE_QUIZ => Some("퀴즈"),
        FEATURE_MODE_VIDEO => Some("동영상 만들기"),
        FEATURE_MODE_IMAGE => Some("이미지 생성"),
        FEATURE_MODE_MUSIC => Some("음악 만들기"),
        FEATURE_MODE_GUIDED_LEARNING => Some("가이드 학습"),
        _ => None,
    }
}

fn main() -> iced::Result {
    let launch_mode = match tray::LaunchMode::from_args(std::env::args()) {
        Ok(mode) => mode,
        Err(help) => {
            print!("{help}");
            return Ok(());
        }
    };

    if tray::try_activate_existing_instance(launch_mode) {
        return Ok(());
    }

    let start_hidden = launch_mode == tray::LaunchMode::StartInTray;
    iced::daemon(move || App::new(start_hidden), App::update, App::view)
        .title(App::title)
        .subscription(App::subscription)
        .theme(App::theme)
        .default_font(font_regular())
        .run()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Role {
    User,
    Model,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ActiveSection {
    Chat,
    Library,
    Images,
    Schedules,
    Student,
    Notebooks,
    GemsHub,
    Videos,
    Personalize,
    Gem(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ComposerMode {
    Chat,
    Task,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ConvFilter {
    All,
    Ready,
    NeedsAttention,
    InProgress,
    PinnedOnly,
}

impl ConvFilter {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::All => "전체",
            Self::Ready => "준비됨",
            Self::NeedsAttention => "확인 필요",
            Self::InProgress => "진행 중",
            Self::PinnedOnly => "고정됨",
        }
    }
}

struct ChatMessage {
    role: Role,
    raw_text: String,
    markdown: markdown_view::ParsedMarkdown,
    thoughts: String,
    thoughts_expanded: bool,
    model_name: String,
    request_id: String,
    response_id: String,
    status_text: String,
    tool_activities: Vec<RemoteToolActivity>,
    tools_expanded: bool,
    target_device_name: String,
    attachments: Vec<String>,
    attachment_items: Vec<TurnAttachment>,
    generated_images: Vec<GeneratedImage>,
    artifacts: Vec<TurnArtifact>,
    is_scheduled_run: bool,
    scheduled_title: String,
    aborted: bool,
    timestamp_unix: i64,
}

impl ChatMessage {
    fn user(text: String, attachments: Vec<String>) -> Self {
        Self {
            role: Role::User,
            raw_text: text,
            markdown: markdown_view::ParsedMarkdown::empty(),
            thoughts: String::new(),
            thoughts_expanded: false,
            model_name: String::new(),
            request_id: String::new(),
            response_id: String::new(),
            status_text: String::new(),
            tool_activities: Vec::new(),
            tools_expanded: false,
            target_device_name: String::new(),
            attachments,
            attachment_items: Vec::new(),
            generated_images: Vec::new(),
            artifacts: Vec::new(),
            is_scheduled_run: false,
            scheduled_title: String::new(),
            aborted: false,
            timestamp_unix: 0,
        }
    }

    fn model(text: String, thoughts: String, model_name: String) -> Self {
        let md = markdown_view::ParsedMarkdown::parse(&text);
        Self {
            role: Role::Model,
            raw_text: text,
            markdown: md,
            thoughts,
            thoughts_expanded: false,
            model_name,
            request_id: String::new(),
            response_id: String::new(),
            status_text: String::new(),
            tool_activities: Vec::new(),
            tools_expanded: false,
            target_device_name: String::new(),
            attachments: Vec::new(),
            attachment_items: Vec::new(),
            generated_images: Vec::new(),
            artifacts: Vec::new(),
            is_scheduled_run: false,
            scheduled_title: String::new(),
            aborted: false,
            timestamp_unix: 0,
        }
    }

    fn push_delta(&mut self, delta: &str, full_text: &str, defer_reparse: bool) -> bool {
        if !delta.is_empty() && (full_text.is_empty() || full_text.starts_with(&self.raw_text)) {
            let prev_ended_star = self.raw_text.ends_with('*');
            self.raw_text.push_str(delta);
            self.markdown
                .push_delta(prev_ended_star, delta, &self.raw_text, defer_reparse)
        } else if !full_text.is_empty() && full_text != self.raw_text {
            self.raw_text = full_text.to_string();
            if defer_reparse {
                true
            } else {
                self.markdown = markdown_view::ParsedMarkdown::parse(&self.raw_text);
                false
            }
        } else {
            false
        }
    }
}

struct CanvasPreviewState {
    msg_idx: usize,
    art_idx: usize,
    artifact: TurnArtifact,
    parsed_markdown: Option<markdown_view::ParsedMarkdown>,
    show_raw: bool,
}

pub(crate) struct App {
    pub(crate) palette: AppTheme,
    cached_iced_theme: Theme,
    pub(crate) theme_mode: ThemeMode,
    pub(crate) language_pref: LanguagePref,
    pub(crate) locale: Locale,
    system_dark_fallback: bool,

    // 데몬 · 계정 프로필 · 등록 기기 · 원격 제어 상태
    daemon_connected: bool,
    daemon_label: String,
    profile: UserProfile,
    avatar_handle: Option<image::Handle>,
    devices: Vec<RegisteredDevice>,
    pub(crate) remote_status: RemoteStatusSnapshot,
    selected_device_id: Option<String>,
    device_menu_open: bool,
    remote_busy: bool,
    folder_input_draft: String,
    usage_info: Option<UsageInfo>,

    // 내비게이션 · 사이드바 · 팝오버 · 모달
    sidebar_open: bool,
    pub(crate) sidebar_width: f32,
    sidebar_resizing: bool,
    sidebar_resize_hovered: bool,
    sidebar_resize_start_width: Option<f32>,
    window_width: f32,
    pub(crate) active_section: ActiveSection,
    pub(crate) more_menu_open: bool,
    pub(crate) filter_menu_open: bool,
    pub(crate) conv_filter: ConvFilter,
    pub(crate) conv_menu_target: Option<String>,
    pub(crate) conv_menu_notebook_sub: bool,
    search_modal_open: bool,
    search_query: String,
    search_hits: Vec<SearchHit>,
    search_loading: bool,
    settings_modal_open: bool,
    canvas_preview: Option<CanvasPreviewState>,
    local_tz_offset_secs: i64,

    // 모델 선택기 · 사고 수준 · 도구/기능 모드 · 맞춤설정
    mode_picker_title: String,
    models: Vec<BardModeOption>,
    selected_model_id: Option<String>,
    model_menu_open: bool,
    pub(crate) thinking_level: i32,
    pub(crate) thinking_submenu_open: bool,
    pub(crate) tools_menu_open: bool,
    pub(crate) selected_feature_mode: i32,
    pub(crate) personal_intelligence: bool,
    pub(crate) custom_instructions_draft: String,
    pub(crate) mention_query: Option<String>,

    // Gems / 노트북 / 예약 / 라이브러리
    pub(crate) bots: Vec<BotSummary>,
    active_bot_id: Option<String>,
    pub(crate) notebooks: Vec<Notebook>,
    pub(crate) editing_notebook: Notebook,
    pub(crate) notebook_notes_editor: text_editor::Content,
    pub(crate) schedules: Vec<ScheduledTask>,
    pub(crate) recommended_schedules: Vec<RecommendedSchedule>,
    pub(crate) sched_filter: String,
    pub(crate) sched_title_draft: String,
    pub(crate) sched_prompt_draft: String,
    pub(crate) sched_type_draft: String,
    pub(crate) sched_time_draft: String,
    pub(crate) running_schedule_id: Option<String>,
    pub(crate) library_items: Vec<LibraryEntry>,
    pub(crate) library_filter: String,
    pub(crate) library_image_cache: HashMap<String, image::Handle>,
    library_image_order: VecDeque<String>,
    failed_image_keys: HashSet<String>,
    inflight_image_keys: HashSet<String>,

    // 이미지 · 동영상 스튜디오 상태
    pub(crate) image_prompt_draft: String,
    pub(crate) image_aspect: String,
    pub(crate) image_style: String,
    pub(crate) video_prompt_draft: String,
    pub(crate) video_aspect: String,
    pub(crate) video_duration: String,

    // 대화 상태
    pub(crate) conversations: Vec<ConversationSummary>,
    conversations_next_page_token: Option<String>,
    loading_more_conversations: bool,
    active_conv_id: Option<String>,
    active_conv_is_agent: bool,
    active_project_id: Option<String>,
    active_title: String,
    active_model: String,
    parent_request_id: String,
    parent_response_id: String,
    pending_scroll_request_id: Option<String>,
    messages: Vec<ChatMessage>,
    message_feedback: HashMap<usize, bool>,

    // 인라인 이름 변경
    renaming_conv_id: Option<String>,
    rename_draft: String,

    // 입력창 · 모드 · 첨부 · 화면 공유 · 마이크 녹음
    composer: text_editor::Content,
    composer_has_text: bool,
    pub(crate) composer_mode: ComposerMode,
    attached_files: Vec<PathBuf>,
    file_hovering: bool,
    temporary_chat: bool,
    screen_capturing: bool,
    voice_recorder: Option<VoiceRecorder>,
    voice_stopping: bool,
    recording_seconds: u32,

    loading_history: bool,
    conv_load_seq: u64,
    is_streaming: bool,
    streaming_req_id: String,
    stream_abort_handle: Option<task::Handle>,
    last_stream_reparse: Option<Instant>,
    last_stream_scroll: Option<Instant>,
    stream_markdown_dirty: bool,
    stream_had_attachments: bool,

    // 캔버스 지오메트리 캐시 (테마/모드 변경 시 무효화)
    welcome_glow_cache: canvas::Cache,
    avatar_mask_cache: canvas::Cache,

    // 창 생명주기 · 시스템 트레이
    main_window: Option<window::Id>,
    close_to_tray: bool,
    tray_available: bool,
    tray_tx: Option<tray::TrayUpdateSender>,

    error_banner: Option<String>,
    toast: Option<(String, u8)>,
}

#[derive(Debug, Clone)]
pub(crate) enum Message {
    Tick,
    OmarchyThemeChanged(String),
    SystemThemeChanged(iced::theme::Mode),
    RuntimeEvent(Event),
    WindowOpened(window::Id),
    WindowCloseRequested(window::Id),
    WindowClosed(window::Id),
    TrayEventReceived(tray::TrayEvent),
    ToggleCloseToTray,

    // 초기 및 동기화 응답
    HealthLoaded(Result<HealthResponse, String>),
    ProfileLoaded(Result<UserProfile, String>),
    AvatarLoaded(Result<Vec<u8>, String>),
    ModelsLoaded(Result<ModelsResponse, String>),
    BotsLoaded(Result<BotsResponse, String>),
    DevicesLoaded(Result<DevicesResponse, String>),
    RemoteStatusLoaded(Result<RemoteStatusSnapshot, String>),
    RemoteEventReceived(RemoteEvent),
    ConversationsLoaded(Result<ListConversationsResponse, String>),
    LoadMoreConversations,
    MoreConversationsLoaded(Result<ListConversationsResponse, String>),
    NotebooksLoaded(Result<NotebooksResponse, String>),
    SchedulesLoaded(Result<SchedulesResponse, String>),
    LibraryLoaded(Result<LibraryResponse, String>),
    LibraryImageLoaded(String, Result<Vec<u8>, String>),
    PreferencesLoaded(Result<UIPreferences, String>),
    PreferencesSaved(Result<UIPreferences, String>),
    UsageLoaded(Result<UsageInfo, String>),
    RefreshAll,

    // 원격 PC 선택 · 제어
    ToggleDeviceMenu,
    CloseDeviceMenu,
    SelectDevice(String),
    ToggleRemoteEnabled,
    SyncRemoteRegistration,
    PickRemoteFolder,
    RemoteFolderPicked(Result<Option<PathBuf>, String>),
    FolderDraftChanged(String),
    AddRemoteFolderManual,
    RemoveRemoteFolder(String),
    RemoteActionDone(String, Result<RemoteStatusSnapshot, String>),

    // 사이드바 · 내비게이션 · 필터 · 팝오버
    ToggleSidebar,
    StartSidebarResize,
    SidebarResizeHover(bool),
    ResetSidebarWidth,
    SidebarResizeEvent(Event),
    SwitchSection(ActiveSection),
    ToggleMoreMenu,
    CloseMoreMenu,
    ToggleFilterMenu,
    CloseFilterMenu,
    SetConvFilter(ConvFilter),
    OpenConvMenu(String),
    CloseConvMenu,
    ToggleConvMenuNotebookSub,

    // 검색 모달
    OpenSearchModal,
    CloseSearchModal,
    SearchQueryChanged(String),
    SubmitServerSearch,
    ServerSearchLoaded(Result<SearchResponse, String>),

    // 설정 모달 · 환경설정 · 맞춤설정
    OpenSettingsModal,
    CloseSettingsModal,
    SetThemeMode(ThemeMode),
    SetLanguagePref(LanguagePref),
    TogglePersonalIntelligence,
    CustomInstructionsChanged(String),
    SaveCustomInstructions,

    // 모델 선택기 · 사고 수준 · 도구 팝오버 · @멘션
    ToggleModelMenu,
    CloseModelMenu,
    SelectModel(String),
    ToggleThinkingSubmenu,
    SelectThinkingLevel(i32),
    ToggleToolsMenu,
    CloseToolsMenu,
    SelectFeatureMode(i32),
    ClearFeatureMode,
    InsertMention(String),
    CloseMentionMenu,

    // 대화 선택 · 관리 · 공유 · 읽지않음 · 피드백
    SelectConversation(String, String),
    SelectSearchHit(String, String, String),
    TurnsLoaded(u64, String, Result<ListTurnsResponse, String>),
    NewChat,
    StartRename(String, String),
    RenameDraftChanged(String),
    SubmitRename,
    CancelRename,
    ConversationRenamed(Result<(String, String), String>),
    TogglePinConversation(String, bool),
    ConversationPinned(Result<(String, bool), String>),
    ToggleUnreadConversation(String, bool),
    ConversationUnreadUpdated(Result<(String, bool), String>),
    ConversationUnreadSynced(String, Result<(String, bool), String>),
    ShareConversation(String, String),
    ConversationShared(Result<ShareResponse, String>),
    AddConvToNotebook(String, String),
    ConvAddedToNotebook(Result<(), String>),
    DeleteConversation(String),
    ConversationDeleted(Result<String, String>),
    SendFeedback(usize, bool),
    FeedbackSent(usize, Option<bool>, Result<(), String>),

    // 노트북
    NewNotebook,
    OpenNotebook(String),
    NotebookTitleChanged(String),
    NotebookDescChanged(String),
    NotebookNotesAction(text_editor::Action),
    AddNotebookSource,
    NotebookSourcesPicked(Result<Vec<PathBuf>, String>),
    RemoveNotebookSource(usize),
    SaveNotebook,
    NotebookSaved(Result<Notebook, String>),
    DeleteNotebook(String),
    NotebookDeleted(Result<String, String>),
    ChatWithNotebook,

    // 예약 작업
    SetSchedFilter(String),
    SchedTitleChanged(String),
    SchedPromptChanged(String),
    SchedTypeChanged(String),
    SchedTimeChanged(String),
    CreateSchedule,
    QuickCreateSchedule,
    AddRecommendedSchedule(String, String, String, String),
    RecommendedScheduleAdded(Result<String, String>),
    ScheduleCreated(Result<ScheduledTask, String>),
    ToggleScheduleEnabled(String, bool),
    SchedulePatched(Result<ScheduledTask, String>),
    DeleteSchedule(String),
    ScheduleDeleted(Result<String, String>),
    RunScheduleNow(String),
    ScheduleRunFinished(String, Result<ScheduleRunResult, String>),

    // 라이브러리 · 이미지 · 동영상 · 학생/Gem
    SetLibraryFilter(String),
    AttachLibraryPath(String),
    ImagePromptChanged(String),
    SetImageAspect(String),
    SetImageStyle(String),
    GenerateImagePrompt,
    VideoPromptChanged(String),
    SetVideoAspect(String),
    SetVideoDuration(String),
    GenerateVideoPrompt,
    StartBotPrompt(String, String),
    StartBotChat(String),
    StartFeaturePrompt(i32, String),

    // 입력창 · 화면 공유 · 마이크 · 스트리밍
    SetComposerMode(ComposerMode),
    ToggleTemporary,
    ToggleMessageThoughts(usize),
    ToggleMessageTools(usize),
    OpenCanvasPreview(usize, usize),
    ToggleCanvasRaw,
    CloseCanvasPreview,
    RunCanvasHtml(usize, usize),
    CanvasHtmlOpened(Result<PathBuf, String>),
    RetryImageLoad(String, String),
    OpenFilePicker,
    FilesPicked(Result<Vec<PathBuf>, String>),
    StartScreenCapture,
    ScreenCaptured(Result<PathBuf, String>),
    ToggleVoiceRecording,
    VoiceRecordStopped(Result<PathBuf, String>),
    EditorAction(text_editor::Action),
    RemoveAttachment(usize),
    SendPrompt,
    RegenerateLast,
    StreamReceived(StreamEvent),
    AbortStream,
    AbortFinished(Result<(), String>),

    CopyText(String),
    CopyChatItem(usize),
    CopyCanvasPreview,
    CopyMarkdownTable(markdown_view::CopyTarget, usize),
    CopyMarkdownCode(markdown_view::CopyTarget, usize, usize),
    LinkClicked(markdown::Uri),
    OpenExternalUrl(String),
    DismissError,
}

impl App {
    fn main_window_settings() -> window::Settings {
        window::Settings {
            size: Size::new(1240.0, 860.0),
            min_size: Some(Size::new(780.0, 560.0)),
            exit_on_close_request: false,
            platform_specific: window::settings::PlatformSpecific {
                application_id: "gemini-ui".into(),
                ..Default::default()
            },
            ..Default::default()
        }
    }

    fn new(start_hidden: bool) -> (Self, Task<Message>) {
        let palette = AppTheme::load_with_mode(ThemeMode::System, false);
        let cached_iced_theme = palette.to_iced_theme();
        let language_pref = LanguagePref::System;
        let locale = language_pref.resolve();
        let (main_window, open_window_task) = if start_hidden {
            (None, Task::none())
        } else {
            let (id, task) = window::open(Self::main_window_settings());
            (Some(id), task.map(Message::WindowOpened))
        };
        let app = Self {
            palette,
            cached_iced_theme,
            theme_mode: ThemeMode::System,
            language_pref,
            locale,
            system_dark_fallback: false,
            daemon_connected: false,
            daemon_label: t(locale, "데몬 연결 중...").into(),
            profile: UserProfile::default(),
            avatar_handle: None,
            devices: Vec::new(),
            remote_status: RemoteStatusSnapshot::default(),
            selected_device_id: None,
            device_menu_open: false,
            remote_busy: false,
            folder_input_draft: String::new(),
            usage_info: None,
            sidebar_open: true,
            sidebar_width: SIDEBAR_DEFAULT_WIDTH,
            sidebar_resizing: false,
            sidebar_resize_hovered: false,
            sidebar_resize_start_width: None,
            window_width: 1240.0,
            active_section: ActiveSection::Chat,
            more_menu_open: false,
            filter_menu_open: false,
            conv_filter: ConvFilter::All,
            conv_menu_target: None,
            conv_menu_notebook_sub: false,
            search_modal_open: false,
            search_query: String::new(),
            search_hits: Vec::new(),
            search_loading: false,
            settings_modal_open: false,
            canvas_preview: None,
            local_tz_offset_secs: detect_local_tz_offset_secs(),
            mode_picker_title: String::new(),
            models: Vec::new(),
            selected_model_id: None,
            model_menu_open: false,
            thinking_level: THINKING_MEDIUM,
            thinking_submenu_open: false,
            tools_menu_open: false,
            selected_feature_mode: 0,
            personal_intelligence: false,
            custom_instructions_draft: String::new(),
            mention_query: None,
            bots: Vec::new(),
            active_bot_id: None,
            notebooks: Vec::new(),
            editing_notebook: Notebook::default(),
            notebook_notes_editor: text_editor::Content::new(),
            schedules: Vec::new(),
            recommended_schedules: Vec::new(),
            sched_filter: "all".into(),
            sched_title_draft: String::new(),
            sched_prompt_draft: String::new(),
            sched_type_draft: "daily".into(),
            sched_time_draft: "09:00".into(),
            running_schedule_id: None,
            library_items: Vec::new(),
            library_filter: "all".into(),
            library_image_cache: HashMap::new(),
            library_image_order: VecDeque::new(),
            failed_image_keys: HashSet::new(),
            inflight_image_keys: HashSet::new(),
            image_prompt_draft: String::new(),
            image_aspect: "1:1".into(),
            image_style: "도시락".into(),
            video_prompt_draft: String::new(),
            video_aspect: "16:9".into(),
            video_duration: "5초".into(),
            conversations: Vec::new(),
            conversations_next_page_token: None,
            loading_more_conversations: false,
            active_conv_id: None,
            active_conv_is_agent: false,
            active_project_id: None,
            active_title: String::new(),
            active_model: String::new(),
            parent_request_id: String::new(),
            parent_response_id: String::new(),
            pending_scroll_request_id: None,
            messages: Vec::new(),
            message_feedback: HashMap::new(),
            renaming_conv_id: None,
            rename_draft: String::new(),
            composer: text_editor::Content::new(),
            composer_has_text: false,
            composer_mode: ComposerMode::Chat,
            attached_files: Vec::new(),
            file_hovering: false,
            temporary_chat: false,
            screen_capturing: false,
            voice_recorder: None,
            voice_stopping: false,
            recording_seconds: 0,
            loading_history: false,
            conv_load_seq: 0,
            is_streaming: false,
            streaming_req_id: String::new(),
            stream_abort_handle: None,
            last_stream_reparse: None,
            last_stream_scroll: None,
            stream_markdown_dirty: false,
            stream_had_attachments: false,
            welcome_glow_cache: canvas::Cache::new(),
            avatar_mask_cache: canvas::Cache::new(),
            main_window,
            close_to_tray: true,
            tray_available: false,
            tray_tx: None,
            error_banner: None,
            toast: None,
        };

        let init = Task::batch([
            open_window_task,
            system::theme().map(Message::SystemThemeChanged),
            Task::perform(ipc::health(), Message::HealthLoaded),
            Task::perform(ipc::get_profile(), Message::ProfileLoaded),
            Task::perform(ipc::get_avatar_bytes(), Message::AvatarLoaded),
            Task::perform(ipc::get_preferences(), Message::PreferencesLoaded),
            Task::perform(ipc::get_usage(), Message::UsageLoaded),
            Task::perform(ipc::list_models(), Message::ModelsLoaded),
            Task::perform(ipc::list_bots(), Message::BotsLoaded),
            Task::perform(ipc::list_devices(), Message::DevicesLoaded),
            Task::perform(ipc::get_remote_status(), Message::RemoteStatusLoaded),
            Task::perform(ipc::list_conversations(60), Message::ConversationsLoaded),
            Task::perform(ipc::list_notebooks(), Message::NotebooksLoaded),
            Task::perform(ipc::list_schedules(), Message::SchedulesLoaded),
        ]);
        (app, init)
    }

    fn title(&self, _window: window::Id) -> String {
        if self.active_title.is_empty() {
            "Gemini".into()
        } else {
            format!("{} — Gemini", self.active_title)
        }
    }

    fn theme(&self, _window: window::Id) -> Theme {
        self.cached_iced_theme.clone()
    }

    fn sync_tray_state(&self) {
        if let Some(tx) = &self.tray_tx {
            tx.sync(
                self.main_window.is_some(),
                self.locale,
                self.daemon_connected,
                self.remote_status.remote_enabled,
                self.remote_status.punctual_connected,
                &self.remote_status.current_tool,
                self.is_streaming,
            );
        }
    }

    fn open_or_focus_window(&mut self) -> Task<Message> {
        if let Some(id) = self.main_window {
            return window::gain_focus(id);
        }
        let (id, open_task) = window::open(Self::main_window_settings());
        self.main_window = Some(id);
        open_task.map(Message::WindowOpened)
    }

    fn hide_window_to_tray(&mut self) -> Task<Message> {
        if let Some(id) = self.main_window.take() {
            self.release_hidden_window_caches();
            window::close(id)
        } else {
            Task::none()
        }
    }

    /// 창을 트레이로 숨길 때 재생성 가능한 이미지·캔버스 캐시와 토스트 타이머를 비워 백그라운드 RAM·깨어남을 줄인다.
    fn release_hidden_window_caches(&mut self) {
        self.library_image_cache.clear();
        self.library_image_order.clear();
        self.failed_image_keys.clear();
        self.clear_canvas_caches();
        self.toast = None;
    }

    fn sync_system_theme_if_needed(&mut self) {
        if self.theme_mode == ThemeMode::System {
            let cur = AppTheme::current_omarchy_generation().unwrap_or_default();
            if cur != self.palette.generation {
                let next = AppTheme::load_with_mode(self.theme_mode, self.system_dark_fallback);
                self.set_palette(next);
            }
        }
    }

    fn preload_active_chat_images(&mut self) -> Task<Message> {
        let mut tasks = Vec::new();
        let mut preload_count = 0usize;
        for msg in &self.messages {
            for att in &msg.attachment_items {
                if preload_count < 24 && is_attachment_image(att) && !att.url.is_empty() {
                    let key = markdown_view::turn_attachment_cache_key(&att.url);
                    if !self.library_image_cache.contains_key(&key)
                        && !self.failed_image_keys.contains(&key)
                        && self.inflight_image_keys.insert(key.clone())
                    {
                        let url = att.url.clone();
                        tasks.push(Task::perform(ipc::fetch_library_image(url), move |r| {
                            Message::LibraryImageLoaded(key, r)
                        }));
                        preload_count += 1;
                    }
                }
            }
            for img in &msg.generated_images {
                if preload_count < 24 && !img.url.is_empty() {
                    let key = markdown_view::generated_image_cache_key(&img.url);
                    if !self.library_image_cache.contains_key(&key)
                        && !self.failed_image_keys.contains(&key)
                        && self.inflight_image_keys.insert(key.clone())
                    {
                        let url = img.url.clone();
                        tasks.push(Task::perform(ipc::fetch_library_image(url), move |r| {
                            Message::LibraryImageLoaded(key, r)
                        }));
                        preload_count += 1;
                    }
                }
            }
            if preload_count >= 24 {
                break;
            }
        }
        if tasks.is_empty() {
            Task::none()
        } else {
            Task::batch(tasks)
        }
    }

    fn preload_visible_images(&mut self) -> Task<Message> {
        match self.active_section {
            ActiveSection::Library | ActiveSection::Images => self.preload_library_images(),
            ActiveSection::Chat => self.preload_active_chat_images(),
            _ => Task::none(),
        }
    }

    fn quit_app(&mut self) -> Task<Message> {
        self.cancel_active_stream();
        if let Some(rec) = self.voice_recorder.take() {
            drop(rec);
        }
        iced::exit()
    }

    fn set_palette(&mut self, next: AppTheme) {
        self.cached_iced_theme = next.to_iced_theme();
        self.palette = next;
        self.clear_canvas_caches();
    }

    fn reset_chat_state(&mut self, clear_bot: bool) {
        self.cancel_active_stream();
        self.active_section = ActiveSection::Chat;
        if clear_bot {
            self.active_bot_id = None;
        }
        self.active_conv_id = None;
        self.active_conv_is_agent = false;
        self.active_project_id = None;
        self.active_title.clear();
        self.active_model.clear();
        self.parent_request_id.clear();
        self.parent_response_id.clear();
        self.pending_scroll_request_id = None;
        self.messages.clear();
        self.message_feedback.clear();
        self.renaming_conv_id = None;
        self.canvas_preview = None;
        self.loading_history = false;
        self.error_banner = None;
        self.conv_load_seq = self.conv_load_seq.wrapping_add(1);
    }

    fn clear_canvas_caches(&mut self) {
        self.welcome_glow_cache.clear();
        self.avatar_mask_cache.clear();
    }

    fn preload_library_images(&mut self) -> Task<Message> {
        let mut tasks = Vec::new();
        for item in &self.library_items {
            if item.kind == "image"
                && !item.url.is_empty()
                && !self.library_image_cache.contains_key(&item.id)
                && !self.failed_image_keys.contains(&item.id)
                && !self.inflight_image_keys.contains(&item.id)
            {
                let id = item.id.clone();
                let url = item.url.clone();
                self.inflight_image_keys.insert(id.clone());
                tasks.push(Task::perform(ipc::fetch_library_image(url), move |r| {
                    Message::LibraryImageLoaded(id, r)
                }));
                if tasks.len() >= 24 {
                    break;
                }
            }
        }
        if tasks.is_empty() {
            Task::none()
        } else {
            Task::batch(tasks)
        }
    }

    fn subscription(&self) -> Subscription<Message> {
        let mut subs = vec![
            system::theme_changes().map(Message::SystemThemeChanged),
            window::close_requests().map(Message::WindowCloseRequested),
            window::close_events().map(Message::WindowClosed),
            Subscription::run(tray::run_tray_service).map(Message::TrayEventReceived),
            event::listen_with(|ev, _status, _id| {
                let interested = match &ev {
                    Event::Window(
                        window::Event::FileHovered(_)
                        | window::Event::FilesHoveredLeft
                        | window::Event::FileDropped(_)
                        | window::Event::Resized(_),
                    ) => true,
                    Event::Keyboard(keyboard::Event::KeyPressed {
                        key, modifiers, ..
                    }) => {
                        if modifiers.control() {
                            matches!(
                                key,
                                keyboard::Key::Character(ch)
                                    if matches!(ch.as_str(), "n" | "N" | "b" | "B" | "k" | "K" | "q" | "Q" | ",")
                            )
                        } else {
                            matches!(key, keyboard::Key::Named(keyboard::key::Named::Escape))
                        }
                    }
                    _ => false,
                };
                if interested {
                    Some(Message::RuntimeEvent(ev))
                } else {
                    None
                }
            }),
            Subscription::run(ipc::stream_remote_events).map(Message::RemoteEventReceived),
        ];

        // 사이드바 구분선 드래그 중일 때만 커서 이동/해제 이벤트를 구독해 평소 유휴 리드로우를 0으로 유지한다.
        if self.sidebar_resizing {
            subs.push(event::listen_with(|ev, _status, _id| {
                let interested = matches!(
                    ev,
                    Event::Mouse(
                        mouse::Event::CursorMoved { .. }
                            | mouse::Event::ButtonReleased(mouse::Button::Left)
                    ) | Event::Window(window::Event::Unfocused)
                );
                if interested {
                    Some(Message::SidebarResizeEvent(ev))
                } else {
                    None
                }
            }));
        }

        // 녹음 중이거나 토스트 표시 중일 때만 1초 타이머를 구동해 유휴 리드로우를 0으로 줄인다.
        if self.voice_recorder.is_some() || self.toast.is_some() {
            subs.push(time::every(Duration::from_secs(1)).map(|_| Message::Tick));
        }

        // 창이 표시 중이고 System 테마 모드일 때만 3초 간격 Omarchy 테마 변경을 감시한다.
        // 트레이 숨김 중에는 주기 깨어남을 0으로 유지하고, 창을 다시 열 때 즉시 1회 동기화한다.
        if self.main_window.is_some() && self.theme_mode == ThemeMode::System {
            subs.push(Subscription::run(watch_omarchy_theme).map(Message::OmarchyThemeChanged));
        }

        Subscription::batch(subs)
    }

    fn scroll_to_bottom() -> Task<Message> {
        operation::snap_to(CHAT_SCROLL_ID, operation::RelativeOffset::END)
    }

    fn show_toast(&mut self, msg: impl Into<String>) {
        self.toast = Some((msg.into(), 3));
    }

    pub(crate) fn picker_models(&self) -> Vec<&BardModeOption> {
        let in_picker: Vec<&BardModeOption> =
            self.models.iter().filter(|m| m.in_picker).collect();
        if !in_picker.is_empty() {
            in_picker
        } else {
            self.models.iter().collect()
        }
    }

    pub(crate) fn selected_model(&self) -> Option<&BardModeOption> {
        if let Some(id) = &self.selected_model_id
            && let Some(found) = self.models.iter().find(|m| &m.id == id)
        {
            return Some(found);
        }
        self.picker_models().into_iter().next()
    }

    pub(crate) fn find_bot(&self, id: &str) -> Option<&BotSummary> {
        self.bots.iter().find(|b| b.id == id)
    }

    /// 서버에 등록된 기기와 이 PC 데몬 상태를 병합한 선택 가능 원격 기기 목록을 반환한다.
    fn available_remote_devices(&self) -> Vec<RegisteredDevice> {
        let mut out: Vec<RegisteredDevice> = self.devices.clone();
        if !self.remote_status.device_id.is_empty() {
            if let Some(local) = out
                .iter_mut()
                .find(|d| d.device_id.eq_ignore_ascii_case(&self.remote_status.device_id))
            {
                local.remote_enabled = self.remote_status.remote_enabled;
                if self.remote_status.tool_count > 0 {
                    local.tool_count = self.remote_status.tool_count;
                }
                local.folders = Some(self.remote_status.connected_folders.clone());
            } else {
                out.insert(
                    0,
                    RegisteredDevice {
                        device_id: self.remote_status.device_id.clone(),
                        device_name: self.remote_status.device_name.clone(),
                        remote_enabled: self.remote_status.remote_enabled,
                        tool_count: self.remote_status.tool_count,
                        folders: Some(self.remote_status.connected_folders.clone()),
                    },
                );
            }
        }
        out
    }

    /// 현재 태스크 모드에서 대상으로 선택된 원격 기기를 반환한다.
    pub(crate) fn selected_target_device(&self) -> Option<RegisteredDevice> {
        let all = self.available_remote_devices();
        if let Some(id) = &self.selected_device_id
            && let Some(found) = all.iter().find(|d| d.device_id.eq_ignore_ascii_case(id))
        {
            return Some(found.clone());
        }
        all.iter()
            .find(|d| d.remote_enabled)
            .or_else(|| all.first())
            .cloned()
    }

    fn ensure_selected_device(&mut self) {
        let all = self.available_remote_devices();
        let valid = self
            .selected_device_id
            .as_ref()
            .is_some_and(|id| all.iter().any(|d| d.device_id.eq_ignore_ascii_case(id)));
        if !valid {
            self.selected_device_id = all
                .iter()
                .find(|d| d.remote_enabled)
                .or_else(|| all.first())
                .map(|d| d.device_id.clone());
        }
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        let task = self.update_inner(message);
        self.sync_tray_state();
        task
    }

    fn update_inner(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::WindowOpened(id) => {
                self.main_window = Some(id);
                self.sync_system_theme_if_needed();
                let mut tasks = Vec::new();
                if self.theme_mode == ThemeMode::System {
                    tasks.push(system::theme().map(Message::SystemThemeChanged));
                }
                tasks.push(self.preload_visible_images());
                if !self.messages.is_empty() {
                    tasks.push(Self::scroll_to_bottom());
                }
                Task::batch(tasks)
            }
            Message::WindowCloseRequested(id) => {
                if self.close_to_tray && self.tray_available {
                    if self.main_window == Some(id) {
                        self.main_window = None;
                        self.release_hidden_window_caches();
                    }
                    window::close(id)
                } else {
                    self.quit_app()
                }
            }
            Message::WindowClosed(id) => {
                if self.main_window == Some(id) {
                    self.main_window = None;
                    self.release_hidden_window_caches();
                }
                Task::none()
            }
            Message::TrayEventReceived(ev) => match ev {
                tray::TrayEvent::Ready { available, updater } => {
                    self.tray_available = available;
                    self.tray_tx = updater;
                    Task::none()
                }
                tray::TrayEvent::WatcherAvailabilityChanged(available) => {
                    self.tray_available = available;
                    Task::none()
                }
                tray::TrayEvent::Action(action) => match action {
                    tray::TrayAction::ShowWindow => self.open_or_focus_window(),
                    tray::TrayAction::ToggleWindow => {
                        if self.main_window.is_some() {
                            self.hide_window_to_tray()
                        } else {
                            self.open_or_focus_window()
                        }
                    }
                    tray::TrayAction::NewChat => {
                        let open = self.open_or_focus_window();
                        let chat = self.update_inner(Message::NewChat);
                        Task::batch([open, chat])
                    }
                    tray::TrayAction::ToggleRemote => {
                        self.update_inner(Message::ToggleRemoteEnabled)
                    }
                    tray::TrayAction::OpenSettings => {
                        let open = self.open_or_focus_window();
                        let settings = self.update_inner(Message::OpenSettingsModal);
                        Task::batch([open, settings])
                    }
                    tray::TrayAction::Quit => self.quit_app(),
                },
            },
            Message::ToggleCloseToTray => {
                self.close_to_tray = !self.close_to_tray;
                let enabled = self.close_to_tray;
                self.show_toast(t(
                    self.locale,
                    if enabled {
                        "창 닫기 시 트레이 최소화를 켰습니다"
                    } else {
                        "창 닫기 시 앱을 바로 종료합니다"
                    },
                ));
                Task::perform(
                    ipc::save_preferences(serde_json::json!({ "close_to_tray": enabled })),
                    Message::PreferencesSaved,
                )
            }
            Message::Tick => {
                if self.voice_recorder.is_some() {
                    self.recording_seconds = self.recording_seconds.saturating_add(1);
                }
                if let Some((_, ttl)) = &mut self.toast {
                    if *ttl <= 1 {
                        self.toast = None;
                    } else {
                        *ttl -= 1;
                    }
                }
                Task::none()
            }
            Message::OmarchyThemeChanged(generation) => {
                if self.theme_mode == ThemeMode::System && generation != self.palette.generation {
                    let next =
                        AppTheme::load_with_mode(self.theme_mode, self.system_dark_fallback);
                    self.set_palette(next);
                }
                Task::none()
            }
            Message::SystemThemeChanged(mode) => {
                self.system_dark_fallback = matches!(mode, iced::theme::Mode::Dark);
                if self.theme_mode == ThemeMode::System {
                    let next =
                        AppTheme::load_with_mode(self.theme_mode, self.system_dark_fallback);
                    self.set_palette(next);
                }
                Task::none()
            }
            Message::RuntimeEvent(ev) => match ev {
                Event::Window(window::Event::Resized(size)) => {
                    self.window_width = size.width;
                    self.sidebar_width = clamp_sidebar_width(self.sidebar_width, self.window_width);
                    Task::none()
                }
                Event::Window(window::Event::FileHovered(_)) => {
                    self.file_hovering = true;
                    Task::none()
                }
                Event::Window(window::Event::FilesHoveredLeft) => {
                    self.file_hovering = false;
                    Task::none()
                }
                Event::Window(window::Event::FileDropped(path)) => {
                    self.file_hovering = false;
                    if path.is_file() && !self.attached_files.contains(&path) {
                        let name = path
                            .file_name()
                            .map(|s| s.to_string_lossy().to_string())
                            .unwrap_or_else(|| path.display().to_string());
                        self.attached_files.push(path);
                        self.show_toast(if self.locale.is_en() {
                            format!("Attached file: {name}")
                        } else {
                            format!("파일 첨부됨: {name}")
                        });
                    }
                    Task::none()
                }
                Event::Keyboard(keyboard::Event::KeyPressed {
                    key, modifiers, ..
                }) => {
                    if modifiers.control() {
                        if let keyboard::Key::Character(ref ch) = key {
                            match ch.as_str() {
                                "n" | "N" => return self.update_inner(Message::NewChat),
                                "b" | "B" => return self.update_inner(Message::ToggleSidebar),
                                "k" | "K" => return self.update_inner(Message::OpenSearchModal),
                                "q" | "Q" => return self.quit_app(),
                                "," => return self.update_inner(Message::OpenSettingsModal),
                                _ => {}
                            }
                        }
                    } else if matches!(key, keyboard::Key::Named(keyboard::key::Named::Escape)) {
                        if self.canvas_preview.is_some() {
                            self.canvas_preview = None;
                            return Task::none();
                        }
                        if self.search_modal_open {
                            self.search_modal_open = false;
                            return Task::none();
                        }
                        if self.settings_modal_open {
                            self.settings_modal_open = false;
                            return Task::none();
                        }
                        if self.tools_menu_open {
                            self.tools_menu_open = false;
                            return Task::none();
                        }
                        if self.model_menu_open {
                            self.model_menu_open = false;
                            self.thinking_submenu_open = false;
                            return Task::none();
                        }
                        if self.device_menu_open {
                            self.device_menu_open = false;
                            return Task::none();
                        }
                        if self.more_menu_open {
                            self.more_menu_open = false;
                            return Task::none();
                        }
                        if self.filter_menu_open {
                            self.filter_menu_open = false;
                            return Task::none();
                        }
                        if self.conv_menu_target.is_some() {
                            self.conv_menu_target = None;
                            self.conv_menu_notebook_sub = false;
                            return Task::none();
                        }
                        if self.mention_query.is_some() {
                            self.mention_query = None;
                            return Task::none();
                        }
                        if self.renaming_conv_id.is_some() {
                            return self.update(Message::CancelRename);
                        }
                        if self.is_streaming {
                            return self.update(Message::AbortStream);
                        }
                    }
                    Task::none()
                }
                _ => Task::none(),
            },
            Message::HealthLoaded(Ok(h)) => {
                self.daemon_connected = h.ok;
                self.daemon_label = if h.ok {
                    h.device_name
                } else {
                    t(self.locale, "데몬 비정상").into()
                };
                Task::none()
            }
            Message::HealthLoaded(Err(e)) => {
                self.daemon_connected = false;
                self.daemon_label = t(self.locale, "데몬 미연결").into();
                self.error_banner = Some(if self.locale.is_en() {
                    format!("{e}\nStart the daemon with `systemctl --user start gemini-remote.service`.")
                } else {
                    format!("{e}\n`systemctl --user start gemini-remote.service`로 데몬을 시작하세요.")
                });
                Task::none()
            }
            Message::ProfileLoaded(Ok(p)) => {
                self.profile = p;
                Task::none()
            }
            Message::ProfileLoaded(Err(_)) => Task::none(),
            Message::AvatarLoaded(Ok(bytes)) => {
                if !bytes.is_empty() {
                    self.avatar_handle = Some(image::Handle::from_bytes(bytes));
                }
                Task::none()
            }
            Message::AvatarLoaded(Err(_)) => Task::none(),
            Message::ModelsLoaded(Ok(res)) => {
                self.mode_picker_title = res.mode_picker_title;
                if self.profile.tier.is_empty() && !res.tier.is_empty() {
                    self.profile.tier = res.tier;
                }
                self.models = res.models;
                let keep_valid = self
                    .selected_model_id
                    .as_ref()
                    .is_some_and(|id| self.models.iter().any(|m| &m.id == id));
                if !keep_valid {
                    self.selected_model_id = self
                        .picker_models()
                        .first()
                        .map(|m| m.id.clone());
                }
                Task::none()
            }
            Message::ModelsLoaded(Err(_)) => Task::none(),
            Message::BotsLoaded(Ok(res)) => {
                self.bots = res.bots;
                Task::none()
            }
            Message::BotsLoaded(Err(_)) => Task::none(),
            Message::DevicesLoaded(Ok(res)) => {
                self.devices = res.devices;
                self.ensure_selected_device();
                Task::none()
            }
            Message::DevicesLoaded(Err(_)) => Task::none(),
            Message::RemoteStatusLoaded(Ok(snap)) => {
                self.remote_status = snap;
                self.ensure_selected_device();
                Task::none()
            }
            Message::RemoteStatusLoaded(Err(_)) => Task::none(),
            Message::RemoteEventReceived(ev) => match ev {
                RemoteEvent::Status(snap) => {
                    self.remote_status = snap;
                    self.ensure_selected_device();
                    Task::none()
                }
                RemoteEvent::Tool(tool_ev) => {
                    if tool_ev.phase == "status_changed" {
                        if tool_ev.tool_name == "conversations_updated" {
                            return Task::perform(
                                ipc::list_conversations(60),
                                Message::ConversationsLoaded,
                            );
                        }
                        if let Some(task_name) = tool_ev.tool_name.strip_prefix("scheduled_task:") {
                            self.show_toast(if self.locale.is_en() {
                                format!("Scheduled task finished: {task_name}")
                            } else {
                                format!("예약 작업 완료: {task_name}")
                            });
                            return Task::batch([
                                Task::perform(
                                    ipc::list_conversations(60),
                                    Message::ConversationsLoaded,
                                ),
                                Task::perform(ipc::list_schedules(), Message::SchedulesLoaded),
                            ]);
                        }
                        return Task::none();
                    }
                    match tool_ev.phase.as_str() {
                        "started" | "start" => {
                            self.remote_status.current_tool = tool_ev.tool_name.clone();
                        }
                        "finished" | "done" | "error" | "session_ended" => {
                            self.remote_status.current_tool.clear();
                        }
                        _ => {}
                    }
                    self.remote_status.recent_events.insert(0, tool_ev);
                    self.remote_status.recent_events.truncate(20);
                    Task::none()
                }
            },
            Message::ToggleDeviceMenu => {
                self.device_menu_open = !self.device_menu_open;
                if self.device_menu_open {
                    self.model_menu_open = false;
                }
                Task::none()
            }
            Message::CloseDeviceMenu => {
                self.device_menu_open = false;
                Task::none()
            }
            Message::SelectDevice(id) => {
                self.selected_device_id = Some(id);
                self.device_menu_open = false;
                Task::none()
            }
            Message::ToggleRemoteEnabled => {
                if self.remote_busy {
                    return Task::none();
                }
                self.remote_busy = true;
                let next = !self.remote_status.remote_enabled;
                let label = t(
                    self.locale,
                    if next {
                        "원격 제어를 켰습니다"
                    } else {
                        "원격 제어를 껐습니다"
                    },
                )
                .to_string();
                Task::perform(ipc::toggle_remote(next), move |res| {
                    Message::RemoteActionDone(label, res)
                })
            }
            Message::SyncRemoteRegistration => {
                if self.remote_busy {
                    return Task::none();
                }
                self.remote_busy = true;
                let msg = t(self.locale, "기기 등록 상태를 동기화했습니다").to_string();
                Task::perform(ipc::register_remote(), move |res| {
                    Message::RemoteActionDone(msg, res)
                })
            }
            Message::PickRemoteFolder => {
                if self.remote_busy {
                    return Task::none();
                }
                Task::perform(filepicker::pick_folder(), Message::RemoteFolderPicked)
            }
            Message::RemoteFolderPicked(Ok(Some(path))) => {
                self.remote_busy = true;
                let path_str = path.to_string_lossy().to_string();
                let msg = t(self.locale, "허용 폴더를 추가했습니다").to_string();
                Task::perform(ipc::add_remote_folder(path_str), move |res| {
                    Message::RemoteActionDone(msg, res)
                })
            }
            Message::RemoteFolderPicked(Ok(None)) => Task::none(),
            Message::RemoteFolderPicked(Err(e)) => {
                self.error_banner = Some(if self.locale.is_en() {
                    format!("Failed to select folder: {e}")
                } else {
                    format!("폴더 선택 실패: {e}")
                });
                Task::none()
            }
            Message::FolderDraftChanged(val) => {
                self.folder_input_draft = val;
                Task::none()
            }
            Message::AddRemoteFolderManual => {
                let raw = self.folder_input_draft.trim().to_string();
                if raw.is_empty() || self.remote_busy {
                    return Task::none();
                }
                self.remote_busy = true;
                self.folder_input_draft.clear();
                let msg = t(self.locale, "허용 폴더를 추가했습니다").to_string();
                Task::perform(ipc::add_remote_folder(raw), move |res| {
                    Message::RemoteActionDone(msg, res)
                })
            }
            Message::RemoveRemoteFolder(path) => {
                if self.remote_busy {
                    return Task::none();
                }
                self.remote_busy = true;
                let msg = t(self.locale, "허용 폴더를 제거했습니다").to_string();
                Task::perform(ipc::remove_remote_folder(path), move |res| {
                    Message::RemoteActionDone(msg, res)
                })
            }
            Message::RemoteActionDone(toast_msg, Ok(snap)) => {
                self.remote_busy = false;
                self.remote_status = snap;
                self.ensure_selected_device();
                self.show_toast(toast_msg);
                Task::perform(ipc::list_devices(), Message::DevicesLoaded)
            }
            Message::RemoteActionDone(_, Err(e)) => {
                self.remote_busy = false;
                self.error_banner = Some(if self.locale.is_en() {
                    format!("Failed to update remote control: {e}")
                } else {
                    format!("원격 제어 변경 실패: {e}")
                });
                Task::none()
            }
            Message::NotebooksLoaded(Ok(res)) => {
                self.notebooks = res.notebooks;
                Task::none()
            }
            Message::NotebooksLoaded(Err(_)) => Task::none(),
            Message::SchedulesLoaded(Ok(res)) => {
                self.schedules = res.schedules;
                self.recommended_schedules = res.recommended;
                Task::none()
            }
            Message::SchedulesLoaded(Err(_)) => Task::none(),
            Message::LibraryLoaded(Ok(res)) => {
                self.library_items = res.items;
                if self.main_window.is_some()
                    && matches!(
                        self.active_section,
                        ActiveSection::Library | ActiveSection::Images
                    )
                {
                    self.preload_library_images()
                } else {
                    Task::none()
                }
            }
            Message::LibraryLoaded(Err(_)) => Task::none(),
            Message::LibraryImageLoaded(id, Ok(bytes)) => {
                self.inflight_image_keys.remove(&id);
                if self.main_window.is_none() {
                    return Task::none();
                }
                if bytes.is_empty() {
                    self.failed_image_keys.insert(id);
                } else {
                    self.failed_image_keys.remove(&id);
                    self.cache_library_image(id, image::Handle::from_bytes(bytes));
                }
                Task::none()
            }
            Message::LibraryImageLoaded(id, Err(_)) => {
                self.inflight_image_keys.remove(&id);
                self.failed_image_keys.insert(id);
                Task::none()
            }
            Message::RetryImageLoad(key, url) => {
                self.failed_image_keys.remove(&key);
                if url.trim().is_empty() || !self.inflight_image_keys.insert(key.clone()) {
                    return Task::none();
                }
                Task::perform(ipc::fetch_library_image(url), move |r| {
                    Message::LibraryImageLoaded(key, r)
                })
            }
            Message::PreferencesLoaded(Ok(prefs)) => {
                if !prefs.selected_model_id.is_empty() {
                    self.selected_model_id = Some(prefs.selected_model_id);
                }
                if prefs.thinking_level > 0 {
                    self.thinking_level = prefs.thinking_level;
                }
                let next_mode = ThemeMode::from_str(&prefs.theme_mode);
                if next_mode != self.theme_mode {
                    self.theme_mode = next_mode;
                    let next =
                        AppTheme::load_with_mode(self.theme_mode, self.system_dark_fallback);
                    self.set_palette(next);
                }
                let next_lang = LanguagePref::from_str(&prefs.language);
                self.language_pref = next_lang;
                self.locale = next_lang.resolve();
                self.personal_intelligence = prefs.personal_intelligence;
                self.custom_instructions_draft = prefs.custom_instructions;
                self.close_to_tray = prefs.close_to_tray;
                if prefs.sidebar_width > 0 {
                    self.sidebar_width =
                        clamp_sidebar_width(prefs.sidebar_width as f32, self.window_width);
                }
                Task::none()
            }
            Message::PreferencesLoaded(Err(_)) => Task::none(),
            Message::PreferencesSaved(Ok(_)) => Task::none(),
            Message::PreferencesSaved(Err(e)) => {
                self.error_banner = Some(if self.locale.is_en() {
                    format!("Failed to save preferences: {e}")
                } else {
                    format!("환경설정 저장 실패: {e}")
                });
                Task::none()
            }
            Message::UsageLoaded(Ok(u)) => {
                self.usage_info = Some(u);
                Task::none()
            }
            Message::UsageLoaded(Err(_)) => Task::none(),
            Message::RefreshAll => {
                self.error_banner = None;
                self.model_menu_open = false;
                self.thinking_submenu_open = false;
                self.device_menu_open = false;
                self.tools_menu_open = false;
                self.more_menu_open = false;
                self.filter_menu_open = false;
                self.conv_menu_target = None;
                Task::batch([
                    Task::perform(ipc::health(), Message::HealthLoaded),
                    Task::perform(ipc::get_profile(), Message::ProfileLoaded),
                    Task::perform(ipc::get_usage(), Message::UsageLoaded),
                    Task::perform(ipc::list_models(), Message::ModelsLoaded),
                    Task::perform(ipc::list_bots(), Message::BotsLoaded),
                    Task::perform(ipc::list_devices(), Message::DevicesLoaded),
                    Task::perform(ipc::get_remote_status(), Message::RemoteStatusLoaded),
                    Task::perform(ipc::list_conversations(60), Message::ConversationsLoaded),
                    Task::perform(ipc::list_notebooks(), Message::NotebooksLoaded),
                    Task::perform(ipc::list_schedules(), Message::SchedulesLoaded),
                    Task::perform(ipc::list_library(), Message::LibraryLoaded),
                ])
            }
            Message::ToggleSidebar => {
                self.sidebar_open = !self.sidebar_open;
                if !self.sidebar_open {
                    self.sidebar_resizing = false;
                    self.sidebar_resize_hovered = false;
                    self.sidebar_resize_start_width = None;
                }
                Task::none()
            }
            Message::StartSidebarResize => {
                if self.sidebar_open {
                    self.sidebar_resizing = true;
                    self.sidebar_resize_start_width = Some(self.sidebar_width);
                }
                Task::none()
            }
            Message::SidebarResizeHover(hovered) => {
                self.sidebar_resize_hovered = hovered;
                Task::none()
            }
            Message::ResetSidebarWidth => {
                self.sidebar_resizing = false;
                self.sidebar_resize_start_width = None;
                let next = clamp_sidebar_width(SIDEBAR_DEFAULT_WIDTH, self.window_width);
                if (self.sidebar_width - next).abs() < 0.5 {
                    return Task::none();
                }
                self.sidebar_width = next;
                Task::perform(
                    ipc::save_preferences(serde_json::json!({ "sidebar_width": next.round() as i32 })),
                    Message::PreferencesSaved,
                )
            }
            Message::SidebarResizeEvent(ev) => match ev {
                Event::Mouse(mouse::Event::CursorMoved { position }) => {
                    if self.sidebar_resizing {
                        let next = clamp_sidebar_width(position.x, self.window_width);
                        if (self.sidebar_width - next).abs() >= 1.0 {
                            self.sidebar_width = next;
                        }
                    }
                    Task::none()
                }
                Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left))
                | Event::Window(window::Event::Unfocused) => {
                    if !self.sidebar_resizing {
                        return Task::none();
                    }
                    self.sidebar_resizing = false;
                    let start = self.sidebar_resize_start_width.take();
                    if start.is_some_and(|w| (self.sidebar_width - w).abs() < 1.0) {
                        return Task::none();
                    }
                    let saved_w = self.sidebar_width.round() as i32;
                    Task::perform(
                        ipc::save_preferences(serde_json::json!({ "sidebar_width": saved_w })),
                        Message::PreferencesSaved,
                    )
                }
                _ => Task::none(),
            },
            Message::SwitchSection(sec) => {
                self.model_menu_open = false;
                self.thinking_submenu_open = false;
                self.tools_menu_open = false;
                self.more_menu_open = false;
                self.filter_menu_open = false;
                self.conv_menu_target = None;
                self.active_section = sec.clone();
                match sec {
                    ActiveSection::Library | ActiveSection::Images => Task::batch([
                        Task::perform(ipc::list_library(), Message::LibraryLoaded),
                        self.preload_library_images(),
                    ]),
                    ActiveSection::Schedules => {
                        Task::perform(ipc::list_schedules(), Message::SchedulesLoaded)
                    }
                    ActiveSection::Notebooks => {
                        Task::perform(ipc::list_notebooks(), Message::NotebooksLoaded)
                    }
                    ActiveSection::GemsHub => Task::perform(ipc::list_bots(), Message::BotsLoaded),
                    _ => Task::none(),
                }
            }
            Message::ToggleMoreMenu => {
                self.more_menu_open = !self.more_menu_open;
                if self.more_menu_open {
                    self.filter_menu_open = false;
                    self.conv_menu_target = None;
                }
                Task::none()
            }
            Message::CloseMoreMenu => {
                self.more_menu_open = false;
                Task::none()
            }
            Message::ToggleFilterMenu => {
                self.filter_menu_open = !self.filter_menu_open;
                if self.filter_menu_open {
                    self.more_menu_open = false;
                    self.conv_menu_target = None;
                }
                Task::none()
            }
            Message::CloseFilterMenu => {
                self.filter_menu_open = false;
                Task::none()
            }
            Message::SetConvFilter(f) => {
                self.conv_filter = f;
                self.filter_menu_open = false;
                Task::none()
            }
            Message::OpenConvMenu(conv_id) => {
                let same = self.conv_menu_target.as_deref() == Some(conv_id.as_str());
                self.conv_menu_target = if same { None } else { Some(conv_id) };
                self.conv_menu_notebook_sub = false;
                self.more_menu_open = false;
                self.filter_menu_open = false;
                Task::none()
            }
            Message::CloseConvMenu => {
                self.conv_menu_target = None;
                self.conv_menu_notebook_sub = false;
                Task::none()
            }
            Message::ToggleConvMenuNotebookSub => {
                self.conv_menu_notebook_sub = !self.conv_menu_notebook_sub;
                Task::none()
            }
            Message::OpenSearchModal => {
                self.search_modal_open = true;
                self.model_menu_open = false;
                self.thinking_submenu_open = false;
                self.tools_menu_open = false;
                self.device_menu_open = false;
                self.more_menu_open = false;
                self.filter_menu_open = false;
                self.conv_menu_target = None;
                self.conv_menu_notebook_sub = false;
                self.settings_modal_open = false;
                operation::focus(SEARCH_INPUT_ID)
            }
            Message::CloseSearchModal => {
                self.search_modal_open = false;
                Task::none()
            }
            Message::SearchQueryChanged(q) => {
                self.search_query = q;
                if self.search_query.trim().is_empty() {
                    self.search_hits.clear();
                }
                Task::none()
            }
            Message::SubmitServerSearch => {
                let q = self.search_query.trim().to_string();
                if q.is_empty() {
                    self.search_hits.clear();
                    return Task::none();
                }
                self.search_loading = true;
                Task::perform(ipc::search_conversations(q), Message::ServerSearchLoaded)
            }
            Message::ServerSearchLoaded(Ok(res)) => {
                self.search_loading = false;
                self.search_hits = res.hits;
                Task::none()
            }
            Message::ServerSearchLoaded(Err(e)) => {
                self.search_loading = false;
                self.error_banner = Some(if self.locale.is_en() {
                    format!("Conversation search failed: {e}")
                } else {
                    format!("대화 검색 실패: {e}")
                });
                Task::none()
            }
            Message::OpenSettingsModal => {
                self.settings_modal_open = true;
                self.search_modal_open = false;
                self.model_menu_open = false;
                self.thinking_submenu_open = false;
                self.tools_menu_open = false;
                self.device_menu_open = false;
                self.more_menu_open = false;
                self.filter_menu_open = false;
                self.conv_menu_target = None;
                Task::batch([
                    Task::perform(ipc::list_devices(), Message::DevicesLoaded),
                    Task::perform(ipc::get_remote_status(), Message::RemoteStatusLoaded),
                    Task::perform(ipc::get_usage(), Message::UsageLoaded),
                ])
            }
            Message::CloseSettingsModal => {
                self.settings_modal_open = false;
                Task::none()
            }
            Message::SetThemeMode(mode) => {
                self.theme_mode = mode;
                let next = AppTheme::load_with_mode(mode, self.system_dark_fallback);
                self.set_palette(next);
                let lbl = mode.label_for(self.locale);
                self.show_toast(if self.locale.is_en() {
                    format!("Theme: {lbl}")
                } else {
                    format!("테마: {lbl}")
                });
                Task::perform(
                    ipc::save_preferences(serde_json::json!({ "theme_mode": mode.as_str() })),
                    Message::PreferencesSaved,
                )
            }
            Message::SetLanguagePref(pref) => {
                self.language_pref = pref;
                self.locale = pref.resolve();
                let lbl = pref.label(self.locale);
                self.show_toast(if self.locale.is_en() {
                    format!("Language: {lbl}")
                } else {
                    format!("언어: {lbl}")
                });
                Task::perform(
                    ipc::save_preferences(serde_json::json!({ "language": pref.as_str() })),
                    Message::PreferencesSaved,
                )
            }
            Message::TogglePersonalIntelligence => {
                self.personal_intelligence = !self.personal_intelligence;
                let enabled = self.personal_intelligence;
                self.show_toast(t(
                    self.locale,
                    if enabled {
                        "개인 인텔리전스를 켰습니다"
                    } else {
                        "개인 인텔리전스를 껐습니다"
                    },
                ));
                Task::perform(
                    ipc::save_preferences(serde_json::json!({ "personal_intelligence": enabled })),
                    Message::PreferencesSaved,
                )
            }
            Message::CustomInstructionsChanged(val) => {
                self.custom_instructions_draft = val;
                Task::none()
            }
            Message::SaveCustomInstructions => {
                let text = self.custom_instructions_draft.trim().to_string();
                self.show_toast(t(self.locale, "맞춤 지침을 저장했습니다"));
                Task::perform(
                    ipc::save_preferences(serde_json::json!({ "custom_instructions": text })),
                    Message::PreferencesSaved,
                )
            }
            Message::ToggleModelMenu => {
                self.model_menu_open = !self.model_menu_open;
                self.thinking_submenu_open = false;
                if self.model_menu_open {
                    self.device_menu_open = false;
                    self.tools_menu_open = false;
                }
                Task::none()
            }
            Message::CloseModelMenu => {
                self.model_menu_open = false;
                self.thinking_submenu_open = false;
                Task::none()
            }
            Message::SelectModel(id) => {
                self.selected_model_id = Some(id.clone());
                self.model_menu_open = false;
                self.thinking_submenu_open = false;
                Task::perform(
                    ipc::save_preferences(serde_json::json!({ "selected_model_id": id })),
                    Message::PreferencesSaved,
                )
            }
            Message::ToggleThinkingSubmenu => {
                self.thinking_submenu_open = !self.thinking_submenu_open;
                Task::none()
            }
            Message::SelectThinkingLevel(lv) => {
                self.thinking_level = lv;
                self.thinking_submenu_open = false;
                self.model_menu_open = false;
                let label = t(
                    self.locale,
                    match lv {
                        THINKING_LOW => "낮음",
                        THINKING_HIGH => "높음",
                        _ => "보통",
                    },
                );
                self.show_toast(if self.locale.is_en() {
                    format!("Thinking level: {label}")
                } else {
                    format!("사고 수준: {label}")
                });
                Task::perform(
                    ipc::save_preferences(serde_json::json!({ "thinking_level": lv })),
                    Message::PreferencesSaved,
                )
            }
            Message::ToggleToolsMenu => {
                self.tools_menu_open = !self.tools_menu_open;
                if self.tools_menu_open {
                    self.model_menu_open = false;
                    self.thinking_submenu_open = false;
                    self.device_menu_open = false;
                }
                Task::none()
            }
            Message::CloseToolsMenu => {
                self.tools_menu_open = false;
                Task::none()
            }
            Message::SelectFeatureMode(mode) => {
                self.selected_feature_mode = if self.selected_feature_mode == mode {
                    0
                } else {
                    mode
                };
                self.tools_menu_open = false;
                if let Some(lbl) = feature_mode_label(self.selected_feature_mode) {
                    let loc_lbl = t(self.locale, lbl);
                    self.show_toast(if self.locale.is_en() {
                        format!("Tool enabled: {loc_lbl}")
                    } else {
                        format!("도구 활성화: {loc_lbl}")
                    });
                }
                Task::none()
            }
            Message::ClearFeatureMode => {
                self.selected_feature_mode = 0;
                Task::none()
            }
            Message::InsertMention(name) => {
                let current = self.composer.text();
                let replaced = if let Some(at_idx) = current.rfind('@') {
                    format!("{}@{} ", &current[..at_idx], name)
                } else {
                    format!("{current}@{name} ")
                };
                self.composer = text_editor::Content::with_text(&replaced);
                self.composer_has_text = !replaced.trim().is_empty();
                self.mention_query = None;
                Task::none()
            }
            Message::CloseMentionMenu => {
                self.mention_query = None;
                Task::none()
            }
            Message::ConversationsLoaded(Ok(res)) => {
                self.conversations = res.conversations;
                self.conversations_next_page_token =
                    (!res.next_page_token.is_empty()).then_some(res.next_page_token);
                self.loading_more_conversations = false;
                Task::none()
            }
            Message::ConversationsLoaded(Err(e)) => {
                self.loading_more_conversations = false;
                self.error_banner = Some(if self.locale.is_en() {
                    format!("Failed to load conversations: {e}")
                } else {
                    format!("대화 목록 조회 실패: {e}")
                });
                Task::none()
            }
            Message::LoadMoreConversations => {
                if self.loading_more_conversations {
                    return Task::none();
                }
                let Some(token) = self.conversations_next_page_token.clone() else {
                    return Task::none();
                };
                self.loading_more_conversations = true;
                Task::perform(
                    ipc::list_conversations_page(60, token),
                    Message::MoreConversationsLoaded,
                )
            }
            Message::MoreConversationsLoaded(Ok(res)) => {
                self.loading_more_conversations = false;
                let existing: HashSet<String> = self
                    .conversations
                    .iter()
                    .map(|c| c.conversation_id.clone())
                    .collect();
                for c in res.conversations {
                    if !existing.contains(&c.conversation_id) {
                        self.conversations.push(c);
                    }
                }
                self.conversations_next_page_token =
                    (!res.next_page_token.is_empty()).then_some(res.next_page_token);
                Task::none()
            }
            Message::MoreConversationsLoaded(Err(e)) => {
                self.loading_more_conversations = false;
                self.error_banner = Some(if self.locale.is_en() {
                    format!("Failed to load older chats: {e}")
                } else {
                    format!("이전 대화 추가 조회 실패: {e}")
                });
                Task::none()
            }
            Message::NewChat => {
                self.model_menu_open = false;
                self.reset_chat_state(true);
                Task::none()
            }
            Message::SelectSearchHit(conv_id, title, request_id) => {
                let task = self.update(Message::SelectConversation(conv_id, title));
                self.pending_scroll_request_id = (!request_id.is_empty()).then_some(request_id);
                task
            }
            Message::SelectConversation(conv_id, title) => {
                self.model_menu_open = false;
                self.search_modal_open = false;
                self.temporary_chat = false;
                self.reset_chat_state(true);
                self.active_conv_id = Some(conv_id.clone());
                self.active_title = title;
                self.loading_history = true;

                let mut is_agent = false;
                let mut was_unread = false;
                for c in &mut self.conversations {
                    if c.conversation_id == conv_id {
                        if c.has_unread {
                            was_unread = true;
                        }
                        c.has_unread = false;
                        is_agent = c.is_agent_chat;
                    }
                }
                // 원앱과 동일하게 기존 대화를 열 때 입력창에 강제 Task 칩을 띄우지 않고,
                // 에이전트 스레드 여부만 내부 플래그로 유지한다.
                self.active_conv_is_agent = is_agent;
                self.composer_mode = ComposerMode::Chat;

                let seq = self.conv_load_seq;
                let id_for_cb = conv_id.clone();
                let load_task = Task::perform(ipc::list_turns(conv_id.clone(), 50), move |res| {
                    Message::TurnsLoaded(seq, id_for_cb, res)
                });
                if was_unread {
                    let sync_cid = conv_id.clone();
                    Task::batch([
                        load_task,
                        Task::perform(
                            ipc::mark_conversation_unread(conv_id, false),
                            move |res| Message::ConversationUnreadSynced(sync_cid, res),
                        ),
                    ])
                } else {
                    load_task
                }
            }
            Message::TurnsLoaded(seq, conv_id, Ok(res)) => {
                if seq != self.conv_load_seq
                    || self.is_streaming
                    || self.active_conv_id.as_deref() != Some(conv_id.as_str())
                {
                    return Task::none();
                }
                self.loading_history = false;
                let mut turns: Vec<ConversationTurn> = res.turns;
                if turns.len() > 1
                    && turns[0].timestamp_unix > turns[turns.len() - 1].timestamp_unix
                {
                    turns.reverse();
                }
                self.messages.clear();
                self.message_feedback.clear();
                let mut tasks: Vec<Task<Message>> = vec![Self::scroll_to_bottom()];
                let mut preload_count = 0usize;

                for t in &turns {
                    if !t.user_text.is_empty() || !t.user_attachments.is_empty() {
                        let names: Vec<String> = t
                            .user_attachments
                            .iter()
                            .filter(|a| !a.name.is_empty())
                            .map(|a| a.name.clone())
                            .collect();
                        let mut um = ChatMessage::user(t.user_text.clone(), names);
                        um.timestamp_unix = t.timestamp_unix;
                        um.attachment_items = t.user_attachments.clone();
                        for att in &t.user_attachments {
                            if preload_count < 24
                                && is_attachment_image(att)
                                && !att.url.is_empty()
                            {
                                let key = markdown_view::turn_attachment_cache_key(&att.url);
                                if !self.library_image_cache.contains_key(&key)
                                    && !self.failed_image_keys.contains(&key)
                                    && self.inflight_image_keys.insert(key.clone())
                                {
                                    let url = att.url.clone();
                                    tasks.push(Task::perform(
                                        ipc::fetch_library_image(url),
                                        move |r| Message::LibraryImageLoaded(key, r),
                                    ));
                                    preload_count += 1;
                                }
                            }
                        }
                        self.messages.push(um);
                    }
                    if !t.model_text.is_empty()
                        || !t.thoughts.is_empty()
                        || !t.tool_activities.is_empty()
                        || !t.generated_images.is_empty()
                        || !t.artifacts.is_empty()
                        || t.aborted
                    {
                        let mut m = ChatMessage::model(
                            t.model_text.clone(),
                            t.thoughts.clone(),
                            t.model_attribution.clone(),
                        );
                        m.request_id = t.request_id.clone();
                        m.response_id = t.response_id.clone();
                        m.status_text = t.status_text.clone();
                        m.tool_activities = t.tool_activities.clone();
                        m.generated_images = t.generated_images.clone();
                        m.artifacts = t.artifacts.clone();
                        m.is_scheduled_run = t.is_scheduled_run;
                        m.scheduled_title = t.scheduled_title.clone();
                        m.aborted = t.aborted;
                        m.timestamp_unix = t.timestamp_unix;
                        for img in &t.generated_images {
                            if preload_count < 24 && !img.url.is_empty() {
                                let key = markdown_view::generated_image_cache_key(&img.url);
                                if !self.library_image_cache.contains_key(&key)
                                    && !self.failed_image_keys.contains(&key)
                                    && self.inflight_image_keys.insert(key.clone())
                                {
                                    let url = img.url.clone();
                                    tasks.push(Task::perform(
                                        ipc::fetch_library_image(url),
                                        move |r| Message::LibraryImageLoaded(key, r),
                                    ));
                                    preload_count += 1;
                                }
                            }
                        }
                        self.messages.push(m);
                    }
                }
                if let Some(last) = turns.last() {
                    self.parent_request_id = last.request_id.clone();
                    self.parent_response_id = last.response_id.clone();
                    self.active_model = last.model_attribution.clone();
                }
                if let Some(target_req) = self.pending_scroll_request_id.take()
                    && let Some(pos) = self
                        .messages
                        .iter()
                        .position(|m| m.request_id == target_req)
                    && !self.messages.is_empty()
                {
                    let denom = self.messages.len().saturating_sub(1).max(1) as f32;
                    let ratio = (pos as f32 / denom).clamp(0.0, 1.0);
                    tasks[0] = operation::snap_to(
                        CHAT_SCROLL_ID,
                        operation::RelativeOffset { x: 0.0, y: ratio },
                    );
                }
                Task::batch(tasks)
            }
            Message::TurnsLoaded(seq, conv_id, Err(e)) => {
                if seq == self.conv_load_seq
                    && self.active_conv_id.as_deref() == Some(conv_id.as_str())
                {
                    self.loading_history = false;
                    self.error_banner = Some(if self.locale.is_en() {
                        format!("Failed to load conversation history: {e}")
                    } else {
                        format!("대화 이력 조회 실패: {e}")
                    });
                }
                Task::none()
            }
            Message::StartRename(conv_id, current_title) => {
                self.conv_menu_target = None;
                self.conv_menu_notebook_sub = false;
                self.renaming_conv_id = Some(conv_id);
                self.rename_draft = current_title;
                Task::none()
            }
            Message::RenameDraftChanged(val) => {
                self.rename_draft = val;
                Task::none()
            }
            Message::CancelRename => {
                self.renaming_conv_id = None;
                self.rename_draft.clear();
                Task::none()
            }
            Message::SubmitRename => {
                let Some(conv_id) = self.renaming_conv_id.take() else {
                    return Task::none();
                };
                let title = self.rename_draft.trim().to_string();
                self.rename_draft.clear();
                if title.is_empty() {
                    return Task::none();
                }
                Task::perform(
                    ipc::rename_conversation(conv_id, title),
                    Message::ConversationRenamed,
                )
            }
            Message::ConversationRenamed(Ok((conv_id, new_title))) => {
                for c in &mut self.conversations {
                    if c.conversation_id == conv_id {
                        c.title = new_title.clone();
                    }
                }
                if self.active_conv_id.as_deref() == Some(conv_id.as_str()) {
                    self.active_title = new_title;
                }
                self.show_toast(t(self.locale, "대화 제목이 변경되었습니다"));
                Task::none()
            }
            Message::ConversationRenamed(Err(e)) => {
                self.error_banner = Some(if self.locale.is_en() {
                    format!("Failed to rename chat: {e}")
                } else {
                    format!("제목 변경 실패: {e}")
                });
                Task::none()
            }
            Message::TogglePinConversation(conv_id, next_pinned) => {
                self.conv_menu_target = None;
                Task::perform(
                    ipc::pin_conversation(conv_id, next_pinned),
                    Message::ConversationPinned,
                )
            }
            Message::ConversationPinned(Ok((conv_id, pinned))) => {
                for c in &mut self.conversations {
                    if c.conversation_id == conv_id {
                        c.pinned = pinned;
                    }
                }
                self.conversations.sort_by(|a, b| {
                    b.pinned
                        .cmp(&a.pinned)
                        .then_with(|| b.timestamp_unix.cmp(&a.timestamp_unix))
                });
                self.show_toast(t(
                    self.locale,
                    if pinned {
                        "대화를 상단에 고정했습니다"
                    } else {
                        "대화 고정을 해제했습니다"
                    },
                ));
                Task::none()
            }
            Message::ConversationPinned(Err(e)) => {
                self.error_banner = Some(if self.locale.is_en() {
                    format!("Failed to update pin status: {e}")
                } else {
                    format!("고정 상태 변경 실패: {e}")
                });
                Task::none()
            }
            Message::ToggleUnreadConversation(conv_id, unread) => {
                self.conv_menu_target = None;
                Task::perform(
                    ipc::mark_conversation_unread(conv_id, unread),
                    Message::ConversationUnreadUpdated,
                )
            }
            Message::ConversationUnreadUpdated(Ok((conv_id, unread))) => {
                for c in &mut self.conversations {
                    if c.conversation_id == conv_id {
                        c.has_unread = unread;
                    }
                }
                self.show_toast(t(
                    self.locale,
                    if unread {
                        "읽지 않음으로 표시했습니다"
                    } else {
                        "읽음으로 표시했습니다"
                    },
                ));
                Task::none()
            }
            Message::ConversationUnreadUpdated(Err(e)) => {
                self.error_banner = Some(if self.locale.is_en() {
                    format!("Failed to update read status: {e}")
                } else {
                    format!("읽음 상태 변경 실패: {e}")
                });
                Task::none()
            }
            Message::ConversationUnreadSynced(cid, res) => {
                match res {
                    Ok((updated_id, unread)) => {
                        if let Some(c) = self
                            .conversations
                            .iter_mut()
                            .find(|c| c.conversation_id == updated_id)
                        {
                            c.has_unread = unread;
                        }
                    }
                    Err(_) => {
                        if let Some(c) = self
                            .conversations
                            .iter_mut()
                            .find(|c| c.conversation_id == cid)
                        {
                            c.has_unread = true;
                        }
                    }
                }
                Task::none()
            }
            Message::ShareConversation(conv_id, title) => {
                self.conv_menu_target = None;
                Task::perform(
                    ipc::share_conversation(conv_id, title),
                    Message::ConversationShared,
                )
            }
            Message::ConversationShared(Ok(res)) => {
                let url = res.url;
                self.show_toast(if self.locale.is_en() {
                    format!("Share link copied: {url}")
                } else {
                    format!("공유 링크 복사됨: {url}")
                });
                clipboard::write(url)
            }
            Message::ConversationShared(Err(e)) => {
                self.error_banner = Some(if self.locale.is_en() {
                    format!("Failed to share chat: {e}")
                } else {
                    format!("대화 공유 실패: {e}")
                });
                Task::none()
            }
            Message::AddConvToNotebook(conv_id, nb_id) => {
                self.conv_menu_target = None;
                self.conv_menu_notebook_sub = false;
                Task::perform(
                    ipc::add_conversation_to_notebook(nb_id, conv_id),
                    Message::ConvAddedToNotebook,
                )
            }
            Message::ConvAddedToNotebook(Ok(())) => {
                self.show_toast(t(self.locale, "대화를 노트북에 추가했습니다"));
                Task::perform(ipc::list_notebooks(), Message::NotebooksLoaded)
            }
            Message::ConvAddedToNotebook(Err(e)) => {
                self.error_banner = Some(if self.locale.is_en() {
                    format!("Failed to add to notebook: {e}")
                } else {
                    format!("노트북 추가 실패: {e}")
                });
                Task::none()
            }
            Message::DeleteConversation(conv_id) => {
                self.conv_menu_target = None;
                Task::perform(
                    ipc::delete_conversation(conv_id),
                    Message::ConversationDeleted,
                )
            }
            Message::ConversationDeleted(Ok(deleted_id)) => {
                self.conversations
                    .retain(|c| c.conversation_id != deleted_id);
                if self.active_conv_id.as_deref() == Some(deleted_id.as_str()) {
                    return self.update(Message::NewChat);
                }
                self.show_toast(t(self.locale, "대화가 삭제되었습니다"));
                Task::none()
            }
            Message::ConversationDeleted(Err(e)) => {
                self.error_banner = Some(if self.locale.is_en() {
                    format!("Failed to delete chat: {e}")
                } else {
                    format!("대화 삭제 실패: {e}")
                });
                Task::none()
            }
            Message::SendFeedback(idx, positive) => {
                let prev = self.message_feedback.insert(idx, positive);
                let conv_id = self.active_conv_id.clone().unwrap_or_default();
                let (req_id, resp_id) = self
                    .messages
                    .get(idx)
                    .map(|m| (m.request_id.clone(), m.response_id.clone()))
                    .unwrap_or_default();
                self.show_toast(t(
                    self.locale,
                    if positive {
                        "좋은 응답으로 피드백을 보냈습니다"
                    } else {
                        "개선 필요 피드백을 보냈습니다"
                    },
                ));
                Task::perform(
                    ipc::send_feedback(conv_id, req_id, resp_id, positive),
                    move |res| Message::FeedbackSent(idx, prev, res),
                )
            }
            Message::FeedbackSent(_, _, Ok(())) => Task::none(),
            Message::FeedbackSent(idx, prev, Err(e)) => {
                match prev {
                    Some(v) => {
                        self.message_feedback.insert(idx, v);
                    }
                    None => {
                        self.message_feedback.remove(&idx);
                    }
                }
                self.error_banner = Some(if self.locale.is_en() {
                    format!("Failed to send feedback: {e}")
                } else {
                    format!("피드백 전송 실패: {e}")
                });
                Task::none()
            }

            // --- 노트북 ---
            Message::NewNotebook => {
                self.active_section = ActiveSection::Notebooks;
                self.editing_notebook = Notebook {
                    title: t(self.locale, "새 노트북").into(),
                    ..Notebook::default()
                };
                self.notebook_notes_editor = text_editor::Content::new();
                Task::none()
            }
            Message::OpenNotebook(id) => {
                self.active_section = ActiveSection::Notebooks;
                if let Some(nb) = self.notebooks.iter().find(|n| n.id == id).cloned() {
                    self.notebook_notes_editor = text_editor::Content::with_text(&nb.notes);
                    self.editing_notebook = nb;
                }
                Task::none()
            }
            Message::NotebookTitleChanged(v) => {
                self.editing_notebook.title = v;
                Task::none()
            }
            Message::NotebookDescChanged(v) => {
                self.editing_notebook.description = v;
                Task::none()
            }
            Message::NotebookNotesAction(action) => {
                let is_edit = action.is_edit();
                self.notebook_notes_editor.perform(action);
                if is_edit {
                    self.editing_notebook.notes = self.notebook_notes_editor.text();
                }
                Task::none()
            }
            Message::AddNotebookSource => {
                Task::perform(filepicker::pick_files(), Message::NotebookSourcesPicked)
            }
            Message::NotebookSourcesPicked(Ok(paths)) => {
                for p in paths {
                    let s = p.to_string_lossy().to_string();
                    if !self.editing_notebook.sources.contains(&s) {
                        self.editing_notebook.sources.push(s);
                    }
                }
                Task::none()
            }
            Message::NotebookSourcesPicked(Err(e)) => {
                self.error_banner = Some(if self.locale.is_en() {
                    format!("Failed to pick source files: {e}")
                } else {
                    format!("소스 파일 선택 오류: {e}")
                });
                Task::none()
            }
            Message::RemoveNotebookSource(idx) => {
                if idx < self.editing_notebook.sources.len() {
                    self.editing_notebook.sources.remove(idx);
                }
                Task::none()
            }
            Message::SaveNotebook => {
                let mut nb = self.editing_notebook.clone();
                nb.notes = self.notebook_notes_editor.text();
                Task::perform(ipc::upsert_notebook(nb), Message::NotebookSaved)
            }
            Message::NotebookSaved(Ok(saved)) => {
                self.editing_notebook = saved.clone();
                self.notebooks.retain(|n| n.id != saved.id);
                self.notebooks.insert(0, saved);
                self.show_toast(t(self.locale, "노트북이 저장되었습니다"));
                Task::none()
            }
            Message::NotebookSaved(Err(e)) => {
                self.error_banner = Some(if self.locale.is_en() {
                    format!("Failed to save notebook: {e}")
                } else {
                    format!("노트북 저장 실패: {e}")
                });
                Task::none()
            }
            Message::DeleteNotebook(id) => {
                Task::perform(ipc::delete_notebook(id), Message::NotebookDeleted)
            }
            Message::NotebookDeleted(Ok(id)) => {
                self.notebooks.retain(|n| n.id != id);
                if self.editing_notebook.id == id {
                    self.editing_notebook = Notebook::default();
                    self.notebook_notes_editor = text_editor::Content::new();
                }
                self.show_toast(t(self.locale, "노트북이 삭제되었습니다"));
                Task::none()
            }
            Message::NotebookDeleted(Err(e)) => {
                self.error_banner = Some(if self.locale.is_en() {
                    format!("Failed to delete notebook: {e}")
                } else {
                    format!("노트북 삭제 실패: {e}")
                });
                Task::none()
            }
            Message::ChatWithNotebook => {
                let nb = self.editing_notebook.clone();
                let notes = self.notebook_notes_editor.text().trim().to_string();
                let mut files = Vec::new();
                for src in &nb.sources {
                    let p = PathBuf::from(src);
                    if p.is_file() {
                        files.push(p);
                    }
                }
                self.reset_chat_state(true);
                if nb.is_remote && !nb.id.is_empty() {
                    self.active_project_id = Some(nb.id.clone());
                }
                self.attached_files = files;
                let nb_title = t(self.locale, &nb.title);
                let prompt = if notes.is_empty() {
                    if self.locale.is_en() {
                        format!(
                            "[Notebook: {}] Analyze the attached source materials and summarize the key points.",
                            nb_title
                        )
                    } else {
                        format!(
                            "[노트북: {}] 첨부된 소스 자료를 분석하고 핵심 내용을 요약해 주세요.",
                            nb_title
                        )
                    }
                } else if self.locale.is_en() {
                    format!(
                        "[Notebook: {}]\nNotes context:\n{}\n\nSummarize the key points from these notes and attached sources and get ready for questions.",
                        nb_title, notes
                    )
                } else {
                    format!(
                        "[노트북: {}]\n메모 컨텍스트:\n{}\n\n위 메모와 첨부 소스를 바탕으로 핵심을 정리하고 질문할 준비를 해 주세요.",
                        nb_title, notes
                    )
                };
                self.composer = text_editor::Content::with_text(&prompt);
                self.composer_has_text = !prompt.trim().is_empty();
                self.show_toast(if self.locale.is_en() {
                    format!("Connected sources from notebook '{}' to chat", nb_title)
                } else {
                    format!("노트북 '{}' 소스가 채팅에 연결되었습니다", nb_title)
                });
                Task::none()
            }

            // --- 예약 작업 ---
            Message::SetSchedFilter(f) => {
                self.sched_filter = f;
                Task::none()
            }
            Message::SchedTitleChanged(v) => {
                self.sched_title_draft = v;
                Task::none()
            }
            Message::SchedPromptChanged(v) => {
                self.sched_prompt_draft = v;
                Task::none()
            }
            Message::SchedTypeChanged(v) => {
                self.sched_type_draft = v;
                Task::none()
            }
            Message::SchedTimeChanged(v) => {
                self.sched_time_draft = v;
                Task::none()
            }
            Message::QuickCreateSchedule => {
                if !self.sched_prompt_draft.trim().is_empty() {
                    return self.update(Message::CreateSchedule);
                }
                self.sched_title_draft = t(self.locale, "매일 아침 브리핑").into();
                self.sched_prompt_draft = t(
                    self.locale,
                    "오늘 주요 기술 뉴스와 주목할 AI 업데이트 5가지를 한국어로 요약해 주세요.",
                )
                .into();
                self.sched_type_draft = "daily".into();
                self.sched_time_draft = "09:00".into();
                self.show_toast(t(self.locale, "예약 작업 입력란에 기본 템플릿을 채웠습니다"));
                Task::none()
            }
            Message::AddRecommendedSchedule(title, prompt, sched_type, time_of_day) => {
                Task::perform(
                    ipc::add_recommended_schedule(title, prompt, sched_type, time_of_day),
                    Message::RecommendedScheduleAdded,
                )
            }
            Message::RecommendedScheduleAdded(Ok(title)) => {
                self.show_toast(if self.locale.is_en() {
                    format!("Added recommended schedule: {title}")
                } else {
                    format!("추천 예약 추가됨: {title}")
                });
                Task::perform(ipc::list_schedules(), Message::SchedulesLoaded)
            }
            Message::RecommendedScheduleAdded(Err(e)) => {
                self.error_banner = Some(if self.locale.is_en() {
                    format!("Failed to add recommended schedule: {e}")
                } else {
                    format!("추천 예약 추가 실패: {e}")
                });
                Task::none()
            }
            Message::CreateSchedule => {
                if self.sched_prompt_draft.trim().is_empty() {
                    self.show_toast(t(self.locale, "예약 실행할 프롬프트를 입력해 주세요"));
                    return Task::none();
                }
                let task = ScheduledTask {
                    title: if self.sched_title_draft.trim().is_empty() {
                        t(self.locale, "예약 브리핑").into()
                    } else {
                        self.sched_title_draft.trim().to_string()
                    },
                    prompt: self.sched_prompt_draft.trim().to_string(),
                    schedule_type: self.sched_type_draft.clone(),
                    time_of_day: if self.sched_time_draft.trim().is_empty() {
                        "09:00".into()
                    } else {
                        self.sched_time_draft.trim().to_string()
                    },
                    enabled: true,
                    ..ScheduledTask::default()
                };
                Task::perform(ipc::create_schedule(task), Message::ScheduleCreated)
            }
            Message::ScheduleCreated(Ok(created)) => {
                self.sched_title_draft.clear();
                self.sched_prompt_draft.clear();
                self.schedules.insert(0, created);
                self.show_toast(t(self.locale, "예약 작업이 등록되었습니다"));
                Task::none()
            }
            Message::ScheduleCreated(Err(e)) => {
                self.error_banner = Some(if self.locale.is_en() {
                    format!("Failed to create schedule: {e}")
                } else {
                    format!("예약 생성 실패: {e}")
                });
                Task::none()
            }
            Message::ToggleScheduleEnabled(id, enabled) => {
                Task::perform(ipc::patch_schedule(id, enabled), Message::SchedulePatched)
            }
            Message::SchedulePatched(Ok(updated)) => {
                for s in &mut self.schedules {
                    if s.id == updated.id {
                        *s = updated.clone();
                    }
                }
                Task::none()
            }
            Message::SchedulePatched(Err(e)) => {
                self.error_banner = Some(if self.locale.is_en() {
                    format!("Failed to update schedule: {e}")
                } else {
                    format!("예약 변경 실패: {e}")
                });
                Task::none()
            }
            Message::DeleteSchedule(id) => {
                Task::perform(ipc::delete_schedule(id), Message::ScheduleDeleted)
            }
            Message::ScheduleDeleted(Ok(id)) => {
                self.schedules.retain(|s| s.id != id);
                self.show_toast(t(self.locale, "예약 작업이 삭제되었습니다"));
                Task::none()
            }
            Message::ScheduleDeleted(Err(e)) => {
                self.error_banner = Some(if self.locale.is_en() {
                    format!("Failed to delete schedule: {e}")
                } else {
                    format!("예약 삭제 실패: {e}")
                });
                Task::none()
            }
            Message::RunScheduleNow(id) => {
                self.running_schedule_id = Some(id.clone());
                let id_cb = id.clone();
                Task::perform(ipc::run_schedule_now(id), move |res| {
                    Message::ScheduleRunFinished(id_cb, res)
                })
            }
            Message::ScheduleRunFinished(_id, Ok(res)) => {
                self.running_schedule_id = None;
                let title_str = if res.title.is_empty() {
                    t(self.locale, "대화 저장됨")
                } else {
                    res.title.as_str()
                };
                self.show_toast(if self.locale.is_en() {
                    format!("Schedule run complete: {title_str}")
                } else {
                    format!("예약 실행 완료: {title_str}")
                });
                Task::batch([
                    Task::perform(ipc::list_schedules(), Message::SchedulesLoaded),
                    Task::perform(ipc::list_conversations(60), Message::ConversationsLoaded),
                ])
            }
            Message::ScheduleRunFinished(_id, Err(e)) => {
                self.running_schedule_id = None;
                self.error_banner = Some(if self.locale.is_en() {
                    format!("Failed to run schedule: {e}")
                } else {
                    format!("예약 즉시 실행 실패: {e}")
                });
                Task::none()
            }

            // --- 라이브러리 · 이미지 · 학생/Gem ---
            Message::SetLibraryFilter(f) => {
                self.library_filter = f;
                Task::none()
            }
            Message::AttachLibraryPath(path_str) => {
                let p = PathBuf::from(&path_str);
                if p.is_file() {
                    if !self.attached_files.contains(&p) {
                        self.attached_files.push(p);
                    }
                    self.active_section = ActiveSection::Chat;
                    self.show_toast(t(self.locale, "라이브러리 파일이 입력창에 첨부되었습니다"));
                } else {
                    self.show_toast(t(self.locale, "원본 로컬 파일을 찾을 수 없습니다"));
                }
                Task::none()
            }
            Message::ImagePromptChanged(v) => {
                self.image_prompt_draft = v;
                Task::none()
            }
            Message::SetImageAspect(v) => {
                self.image_aspect = v;
                Task::none()
            }
            Message::SetImageStyle(v) => {
                self.image_style = v;
                Task::none()
            }
            Message::GenerateImagePrompt => {
                let raw = self.image_prompt_draft.trim().to_string();
                if raw.is_empty() {
                    self.show_toast(t(self.locale, "생성할 이미지 설명을 입력해 주세요"));
                    return Task::none();
                }
                self.image_prompt_draft.clear();
                self.reset_chat_state(true);
                self.selected_feature_mode = FEATURE_MODE_IMAGE;
                let style_label = t(self.locale, &self.image_style);
                let full_prompt = if self.locale.is_en() {
                    format!(
                        "Generate a high-quality image based on the following description.\n- Style: {}\n- Aspect ratio: {}\n- Description: {}",
                        style_label, self.image_aspect, raw
                    )
                } else {
                    format!(
                        "다음 설명으로 고품질 이미지를 생성해 주세요.\n- 스타일: {}\n- 화면 비율: {}\n- 설명: {}",
                        style_label, self.image_aspect, raw
                    )
                };
                self.dispatch_prompt(full_prompt, Vec::new())
            }
            Message::VideoPromptChanged(v) => {
                self.video_prompt_draft = v;
                Task::none()
            }
            Message::SetVideoAspect(v) => {
                self.video_aspect = v;
                Task::none()
            }
            Message::SetVideoDuration(v) => {
                self.video_duration = v;
                Task::none()
            }
            Message::GenerateVideoPrompt => {
                let raw = self.video_prompt_draft.trim().to_string();
                if raw.is_empty() {
                    self.show_toast(t(self.locale, "생성할 동영상 장면을 입력해 주세요"));
                    return Task::none();
                }
                self.video_prompt_draft.clear();
                self.reset_chat_state(true);
                self.selected_feature_mode = FEATURE_MODE_VIDEO;
                let dur_label = t(self.locale, &self.video_duration);
                let full_prompt = if self.locale.is_en() {
                    format!(
                        "Generate a video for the following scene.\n- Aspect ratio: {}\n- Duration: {}\n- Scene: {}",
                        self.video_aspect, dur_label, raw
                    )
                } else {
                    format!(
                        "다음 장면으로 동영상을 생성해 주세요.\n- 화면 비율: {}\n- 길이: {}\n- 장면 설명: {}",
                        self.video_aspect, dur_label, raw
                    )
                };
                self.dispatch_prompt(full_prompt, Vec::new())
            }
            Message::StartBotPrompt(bot_id, prompt) => {
                self.reset_chat_state(false);
                self.active_bot_id = Some(bot_id);
                self.dispatch_prompt(prompt, Vec::new())
            }
            Message::StartBotChat(bot_id) => {
                self.reset_chat_state(false);
                self.active_bot_id = Some(bot_id);
                Task::none()
            }
            Message::StartFeaturePrompt(fmode, prompt) => {
                self.reset_chat_state(true);
                self.selected_feature_mode = fmode;
                self.dispatch_prompt(prompt, Vec::new())
            }

            // --- 입력창 · 화면 공유 · 마이크 · 스트리밍 ---
            Message::SetComposerMode(mode) => {
                if self.composer_mode != mode {
                    self.composer_mode = mode;
                    self.welcome_glow_cache.clear();
                }
                Task::none()
            }
            Message::ToggleTemporary => {
                let next = !self.temporary_chat;
                self.model_menu_open = false;
                if next && self.active_conv_id.is_some() {
                    self.reset_chat_state(true);
                }
                self.active_section = ActiveSection::Chat;
                self.temporary_chat = next;
                Task::none()
            }
            Message::ToggleMessageThoughts(idx) => {
                if let Some(msg) = self.messages.get_mut(idx) {
                    msg.thoughts_expanded = !msg.thoughts_expanded;
                }
                Task::none()
            }
            Message::ToggleMessageTools(idx) => {
                if let Some(msg) = self.messages.get_mut(idx) {
                    msg.tools_expanded = !msg.tools_expanded;
                }
                Task::none()
            }
            Message::OpenCanvasPreview(msg_idx, art_idx) => {
                if let Some(art) = self
                    .messages
                    .get(msg_idx)
                    .and_then(|m| m.artifacts.get(art_idx))
                {
                    let parsed_markdown = if markdown_view::is_markdown_artifact(art) {
                        Some(markdown_view::ParsedMarkdown::parse(&art.content))
                    } else {
                        None
                    };
                    self.canvas_preview = Some(CanvasPreviewState {
                        msg_idx,
                        art_idx,
                        artifact: art.clone(),
                        parsed_markdown,
                        show_raw: false,
                    });
                }
                Task::none()
            }
            Message::ToggleCanvasRaw => {
                if let Some(preview) = &mut self.canvas_preview {
                    preview.show_raw = !preview.show_raw;
                }
                Task::none()
            }
            Message::CloseCanvasPreview => {
                self.canvas_preview = None;
                Task::none()
            }
            Message::RunCanvasHtml(msg_idx, art_idx) => {
                if let Some(art) = self
                    .messages
                    .get(msg_idx)
                    .and_then(|m| m.artifacts.get(art_idx))
                {
                    let title = art.title.clone();
                    let content = art.content.clone();
                    return Task::perform(
                        async move {
                            tokio::task::spawn_blocking(move || {
                                write_and_open_canvas_html(&title, &content)
                            })
                            .await
                            .map_err(|e| format!("백그라운드 작업 오류: {e}"))?
                        },
                        Message::CanvasHtmlOpened,
                    );
                }
                Task::none()
            }
            Message::CanvasHtmlOpened(Ok(_)) => {
                self.show_toast(if self.locale.is_en() {
                    "Opened Canvas HTML in browser"
                } else {
                    "브라우저에서 Canvas HTML을 열었습니다"
                });
                Task::none()
            }
            Message::CanvasHtmlOpened(Err(e)) => {
                self.error_banner = Some(if self.locale.is_en() {
                    format!("Failed to launch Canvas HTML: {e}")
                } else {
                    format!("Canvas HTML 실행 실패: {e}")
                });
                Task::none()
            }
            Message::OpenFilePicker => {
                self.model_menu_open = false;
                Task::perform(filepicker::pick_files(), Message::FilesPicked)
            }
            Message::FilesPicked(Ok(paths)) => {
                let mut added = 0usize;
                for p in paths {
                    if !self.attached_files.contains(&p) {
                        self.attached_files.push(p);
                        added += 1;
                    }
                }
                if added == 1 {
                    if let Some(last) = self.attached_files.last() {
                        let name = last
                            .file_name()
                            .map(|s| s.to_string_lossy().to_string())
                            .unwrap_or_else(|| last.display().to_string());
                        self.show_toast(if self.locale.is_en() {
                            format!("Attached file: {name}")
                        } else {
                            format!("파일 첨부됨: {name}")
                        });
                    }
                } else if added > 1 {
                    self.show_toast(if self.locale.is_en() {
                        format!("Attached {added} files")
                    } else {
                        format!("파일 {added}개 첨부됨")
                    });
                }
                Task::none()
            }
            Message::FilesPicked(Err(e)) => {
                self.error_banner = Some(if self.locale.is_en() {
                    format!("File picker error: {e}")
                } else {
                    format!("파일 선택기 오류: {e}")
                });
                Task::none()
            }
            Message::StartScreenCapture => {
                if self.screen_capturing {
                    return Task::none();
                }
                self.screen_capturing = true;
                self.model_menu_open = false;
                Task::perform(capture::capture_screen(), Message::ScreenCaptured)
            }
            Message::ScreenCaptured(Ok(path)) => {
                self.screen_capturing = false;
                if !self.attached_files.contains(&path) {
                    self.attached_files.push(path);
                }
                self.show_toast(t(self.locale, "화면 캡처가 첨부되었습니다"));
                Task::none()
            }
            Message::ScreenCaptured(Err(e)) => {
                self.screen_capturing = false;
                if !e.contains("취소") && !e.to_ascii_lowercase().contains("cancel") {
                    self.error_banner = Some(e);
                }
                Task::none()
            }
            Message::ToggleVoiceRecording => {
                if self.voice_stopping {
                    return Task::none();
                }
                if let Some(recorder) = self.voice_recorder.take() {
                    self.recording_seconds = 0;
                    self.voice_stopping = true;
                    return Task::perform(
                        async move {
                            tokio::task::spawn_blocking(move || recorder.stop())
                                .await
                                .map_err(|e| format!("음성 녹음 종료 작업 오류: {e}"))?
                        },
                        Message::VoiceRecordStopped,
                    );
                } else {
                    match VoiceRecorder::start() {
                        Ok(rec) => {
                            self.voice_recorder = Some(rec);
                            self.recording_seconds = 0;
                            self.show_toast(t(self.locale, "음성 녹음을 시작했습니다 (다시 누르면 종료)"));
                        }
                        Err(e) => {
                            self.error_banner = Some(e);
                        }
                    }
                }
                Task::none()
            }
            Message::VoiceRecordStopped(Ok(wav_path)) => {
                self.voice_stopping = false;
                self.attached_files.push(wav_path);
                self.show_toast(t(self.locale, "음성 녹음이 첨부되었습니다 (전송 시 분석)"));
                Task::none()
            }
            Message::VoiceRecordStopped(Err(e)) => {
                self.voice_stopping = false;
                self.show_toast(e);
                Task::none()
            }
            Message::EditorAction(action) => {
                let is_edit = action.is_edit();
                self.composer.perform(action);
                if is_edit {
                    let current = self.composer.text();
                    self.composer_has_text = !current.trim().is_empty();
                    self.mention_query = extract_mention_query(&current);
                }
                Task::none()
            }
            Message::RemoveAttachment(idx) => {
                if idx < self.attached_files.len() {
                    self.attached_files.remove(idx);
                }
                Task::none()
            }
            Message::SendPrompt => {
                if self.is_streaming {
                    return Task::none();
                }
                let prompt = self.composer.text().trim().to_string();
                if prompt.is_empty() && self.attached_files.is_empty() {
                    return Task::none();
                }
                self.composer = text_editor::Content::new();
                self.composer_has_text = false;
                let files = std::mem::take(&mut self.attached_files);
                self.active_section = ActiveSection::Chat;
                self.dispatch_prompt(prompt, files)
            }
            Message::RegenerateLast => {
                if self.is_streaming {
                    return Task::none();
                }
                if self.messages.last().is_some_and(|m| m.role == Role::Model) {
                    let removed_idx = self.messages.len() - 1;
                    self.messages.pop();
                    self.message_feedback.remove(&removed_idx);
                }
                let prompt = if self.messages.last().is_some_and(|m| m.role == Role::User) {
                    self.messages.pop().map(|m| m.raw_text)
                } else {
                    self.messages
                        .iter()
                        .rev()
                        .find(|m| m.role == Role::User)
                        .map(|m| m.raw_text.clone())
                };
                if let Some(prev_model) = self.messages.iter().rev().find(|m| m.role == Role::Model) {
                    self.parent_request_id = prev_model.request_id.clone();
                    self.parent_response_id = prev_model.response_id.clone();
                } else {
                    self.parent_request_id.clear();
                    self.parent_response_id.clear();
                }
                if let Some(prompt) = prompt {
                    return self.dispatch_prompt(prompt, Vec::new());
                }
                Task::none()
            }
            Message::StreamReceived(ev) => match ev {
                StreamEvent::Chunk(ch) => {
                    self.apply_chunk(&ch, false);
                    const STREAM_SCROLL_INTERVAL: Duration = Duration::from_millis(60);
                    let now = Instant::now();
                    if self
                        .last_stream_scroll
                        .is_none_or(|prev| now.duration_since(prev) >= STREAM_SCROLL_INTERVAL)
                    {
                        self.last_stream_scroll = Some(now);
                        Self::scroll_to_bottom()
                    } else {
                        Task::none()
                    }
                }
                StreamEvent::Done(ch) => {
                    self.apply_chunk(&ch, true);
                    self.is_streaming = false;
                    self.stream_abort_handle = None;
                    let had_attachments = std::mem::take(&mut self.stream_had_attachments);
                    if !self.temporary_chat {
                        let mut tasks = vec![
                            Task::perform(
                                ipc::list_conversations(60),
                                Message::ConversationsLoaded,
                            ),
                            Self::scroll_to_bottom(),
                        ];
                        // 방금 완료된 응답에 생성 이미지 플레이스홀더나 미보강 Canvas 칩이 있으면
                        // 최신 턴을 재조회해 실제 생성 이미지 URL·아티팩트를 즉시 렌더링한다.
                        let (needs_turn_refresh, has_generated_or_artifact) = self
                            .messages
                            .last()
                            .filter(|m| m.role == Role::Model)
                            .map(|m| {
                                let has_img_ref = m
                                    .raw_text
                                    .contains("googleusercontent.com/image_generation_content/");
                                let has_canvas_ref = m
                                    .raw_text
                                    .contains("googleusercontent.com/immersive_entry_chip/");
                                let needs_refresh = (has_img_ref && m.generated_images.is_empty())
                                    || (has_canvas_ref && m.artifacts.is_empty());
                                let has_media = has_img_ref
                                    || has_canvas_ref
                                    || !m.generated_images.is_empty()
                                    || !m.artifacts.is_empty();
                                (needs_refresh, has_media)
                            })
                            .unwrap_or((false, false));

                        if needs_turn_refresh
                            || had_attachments
                            || has_generated_or_artifact
                            || matches!(
                                self.active_section,
                                ActiveSection::Library | ActiveSection::Images
                            )
                        {
                            tasks.push(Task::perform(ipc::list_library(), Message::LibraryLoaded));
                        }
                        if needs_turn_refresh
                            && let Some(conv_id) = self.active_conv_id.clone()
                        {
                            self.conv_load_seq = self.conv_load_seq.wrapping_add(1);
                            let seq = self.conv_load_seq;
                            let id_cb = conv_id.clone();
                            tasks.push(Task::perform(
                                ipc::list_turns(conv_id, 50),
                                move |res| Message::TurnsLoaded(seq, id_cb, res),
                            ));
                        }
                        Task::batch(tasks)
                    } else {
                        Self::scroll_to_bottom()
                    }
                }
                StreamEvent::Error(err) => {
                    self.flush_dirty_stream_markdown();
                    self.is_streaming = false;
                    self.stream_abort_handle = None;
                    self.stream_had_attachments = false;
                    self.error_banner = Some(if self.locale.is_en() {
                        format!("Streaming error: {err}")
                    } else {
                        format!("스트리밍 오류: {err}")
                    });
                    Task::none()
                }
            },
            Message::AbortStream => {
                self.cancel_active_stream();
                self.flush_dirty_stream_markdown();
                if let Some(last) = self.messages.last_mut()
                    && last.role == Role::Model
                {
                    last.aborted = true;
                }
                if let Some(conv_id) = self.active_conv_id.clone()
                    && !self.streaming_req_id.is_empty()
                {
                    let req_id = std::mem::take(&mut self.streaming_req_id);
                    return Task::perform(
                        ipc::abort_generation(conv_id, req_id),
                        Message::AbortFinished,
                    );
                }
                Task::none()
            }
            Message::AbortFinished(Err(e)) => {
                self.error_banner = Some(if self.locale.is_en() {
                    format!("Stop request warning: {e}")
                } else {
                    format!("중단 요청 경고: {e}")
                });
                Task::none()
            }
            Message::AbortFinished(Ok(())) => {
                self.show_toast(t(self.locale, "응답 생성을 중단했습니다"));
                Task::none()
            }
            Message::CopyText(content) => {
                self.show_toast(t(self.locale, "클립보드에 복사되었습니다"));
                clipboard::write(content)
            }
            Message::CopyChatItem(idx) => {
                if let Some(msg) = self.messages.get(idx) {
                    let content = msg.raw_text.clone();
                    self.show_toast(t(self.locale, "클립보드에 복사되었습니다"));
                    clipboard::write(content)
                } else {
                    Task::none()
                }
            }
            Message::CopyCanvasPreview => {
                if let Some(preview) = &self.canvas_preview {
                    let content = preview.artifact.content.clone();
                    self.show_toast(t(self.locale, "클립보드에 복사되었습니다"));
                    clipboard::write(content)
                } else {
                    Task::none()
                }
            }
            Message::CopyMarkdownTable(target, block_idx) => {
                let parsed = match target {
                    markdown_view::CopyTarget::Message(msg_idx) => {
                        self.messages.get(msg_idx).map(|m| &m.markdown)
                    }
                    markdown_view::CopyTarget::CanvasPreview => self
                        .canvas_preview
                        .as_ref()
                        .and_then(|p| p.parsed_markdown.as_ref()),
                };
                if let Some(raw) = parsed.and_then(|p| p.table_raw_markdown(block_idx)) {
                    let content = raw.to_string();
                    self.show_toast(t(self.locale, "클립보드에 복사되었습니다"));
                    clipboard::write(content)
                } else {
                    Task::none()
                }
            }
            Message::CopyMarkdownCode(target, block_idx, code_idx) => {
                let parsed = match target {
                    markdown_view::CopyTarget::Message(msg_idx) => {
                        self.messages.get(msg_idx).map(|m| &m.markdown)
                    }
                    markdown_view::CopyTarget::CanvasPreview => self
                        .canvas_preview
                        .as_ref()
                        .and_then(|p| p.parsed_markdown.as_ref()),
                };
                if let Some(code) = parsed.and_then(|p| p.code_block_text(block_idx, code_idx)) {
                    let content = code.to_string();
                    self.show_toast(t(self.locale, "클립보드에 복사되었습니다"));
                    clipboard::write(content)
                } else {
                    Task::none()
                }
            }
            Message::LinkClicked(url) => {
                if !open_safe_external_url(&url) {
                    self.show_toast(t(self.locale, "안전하지 않은 링크 형식은 열 수 없습니다"));
                }
                Task::none()
            }
            Message::OpenExternalUrl(url) => {
                if !open_safe_external_url(&url) {
                    self.show_toast(t(self.locale, "안전하지 않은 링크 형식은 열 수 없습니다"));
                }
                Task::none()
            }
            Message::DismissError => {
                self.error_banner = None;
                Task::none()
            }
        }
    }

    fn dispatch_prompt(&mut self, prompt: String, files: Vec<PathBuf>) -> Task<Message> {
        self.error_banner = None;
        self.model_menu_open = false;
        self.thinking_submenu_open = false;
        self.tools_menu_open = false;
        self.device_menu_open = false;
        self.mention_query = None;
        self.is_streaming = true;
        self.streaming_req_id.clear();
        self.last_stream_reparse = None;
        self.last_stream_scroll = None;
        self.stream_markdown_dirty = false;
        self.stream_had_attachments = !files.is_empty();

        let file_names: Vec<String> = files
            .iter()
            .map(|p| {
                p.file_name()
                    .map(|s| s.to_string_lossy().to_string())
                    .unwrap_or_else(|| p.display().to_string())
            })
            .collect();
        let file_paths: Vec<String> = files
            .iter()
            .map(|p| p.to_string_lossy().to_string())
            .collect();

        let display_text = if prompt.is_empty() && !file_names.is_empty() {
            let is_audio = file_names.iter().any(|n| n.ends_with(".wav"));
            t(
                self.locale,
                if is_audio {
                    "첨부한 음성 녹음을 듣고 답변해 주세요."
                } else {
                    "첨부한 화면/파일을 확인하고 설명해 주세요."
                },
            )
            .to_string()
        } else {
            prompt
        };

        let is_task = self.composer_mode == ComposerMode::Task;
        let target_dev = if is_task {
            self.selected_target_device()
        } else {
            None
        };
        let target_device_id = target_dev
            .as_ref()
            .map(|d| d.device_id.clone())
            .unwrap_or_default();
        let target_device_name = target_dev
            .as_ref()
            .map(|d| d.device_name.clone())
            .unwrap_or_default();

        self.messages
            .push(ChatMessage::user(display_text.clone(), file_names));
        let mut model_msg = ChatMessage::model(String::new(), String::new(), String::new());
        if is_task && !target_device_name.is_empty() {
            model_msg.target_device_name = target_device_name.clone();
            model_msg.status_text = if self.locale.is_en() {
                format!("Sending remote task to {target_device_name}...")
            } else {
                format!("{target_device_name}에 원격 태스크를 전달하는 중...")
            };
        }
        self.messages.push(model_msg);

        let (mode_category, mode_variant_id, supports_thinking) = match self.selected_model() {
            Some(m) => (m.mode_category, m.id.clone(), m.supports_thinking),
            None => (0, String::new(), false),
        };
        let feature_mode = std::mem::take(&mut self.selected_feature_mode);
        let thinking_level = if supports_thinking {
            self.thinking_level
        } else {
            0
        };

        let req = ChatStreamRequest {
            text: display_text,
            conversation_id: if self.temporary_chat {
                String::new()
            } else {
                self.active_conv_id.clone().unwrap_or_default()
            },
            parent_request_id: if self.temporary_chat {
                String::new()
            } else {
                self.parent_request_id.clone()
            },
            parent_response_id: if self.temporary_chat {
                String::new()
            } else {
                self.parent_response_id.clone()
            },
            temporary: self.temporary_chat,
            bot_id: self.active_bot_id.clone().unwrap_or_default(),
            project_id: self.active_project_id.clone().unwrap_or_default(),
            mode_category,
            mode_variant_id,
            skip_thinking: false,
            thinking_level,
            feature_mode,
            personal_intelligence: self.personal_intelligence,
            agent_task: is_task || self.active_conv_is_agent,
            target_device_id,
            target_device_name,
            file_paths,
        };

        let (stream_task, handle) =
            Task::run(ipc::stream_chat(req), Message::StreamReceived).abortable();
        self.stream_abort_handle = Some(handle);

        Task::batch([stream_task, Self::scroll_to_bottom()])
    }

    fn cancel_active_stream(&mut self) {
        if let Some(handle) = self.stream_abort_handle.take() {
            handle.abort();
        }
        self.is_streaming = false;
    }

    /// 이미지 캐시 크기를 최대 96개로 제한해 장시간 사용 시 메모리 누적을 방지한다.
    fn cache_library_image(&mut self, key: String, handle: image::Handle) {
        const MAX_IMAGE_CACHE_ENTRIES: usize = 96;
        if let Some(pos) = self.library_image_order.iter().position(|k| k == &key) {
            self.library_image_order.remove(pos);
        }
        self.library_image_cache.insert(key.clone(), handle);
        self.library_image_order.push_back(key);
        while self.library_image_order.len() > MAX_IMAGE_CACHE_ENTRIES {
            if let Some(oldest) = self.library_image_order.pop_front() {
                self.library_image_cache.remove(&oldest);
            }
        }
    }

    fn flush_dirty_stream_markdown(&mut self) {
        if self.stream_markdown_dirty {
            if let Some(last) = self.messages.last_mut()
                && last.role == Role::Model
            {
                last.markdown = markdown_view::ParsedMarkdown::parse(&last.raw_text);
            }
            self.stream_markdown_dirty = false;
        }
    }

    fn apply_chunk(&mut self, ch: &ChatStreamChunk, is_done: bool) {
        if !ch.conversation_id.is_empty() && !self.temporary_chat {
            self.active_conv_id = Some(ch.conversation_id.clone());
        }
        if !ch.request_id.is_empty() {
            self.streaming_req_id = ch.request_id.clone();
            if is_done {
                self.parent_request_id = ch.request_id.clone();
            }
        }
        if !ch.response_id.is_empty() && is_done {
            self.parent_response_id = ch.response_id.clone();
        }
        if !ch.title.is_empty() {
            self.active_title = ch.title.clone();
        }
        if !ch.model.is_empty() {
            self.active_model = ch.model.clone();
        }

        if let Some(last) = self.messages.last_mut()
            && last.role == Role::Model
        {
            const STREAM_REPARSE_INTERVAL: Duration = Duration::from_millis(80);
            let now = Instant::now();
            let can_reparse = is_done
                || self
                    .last_stream_reparse
                    .is_none_or(|prev| now.duration_since(prev) >= STREAM_REPARSE_INTERVAL);

            if self.stream_markdown_dirty && can_reparse {
                if !ch.delta.is_empty()
                    && (ch.text.is_empty() || ch.text.starts_with(&last.raw_text))
                {
                    last.raw_text.push_str(&ch.delta);
                } else if !ch.text.is_empty() {
                    last.raw_text = ch.text.clone();
                }
                last.markdown = markdown_view::ParsedMarkdown::parse(&last.raw_text);
                self.stream_markdown_dirty = false;
                self.last_stream_reparse = Some(now);
            } else {
                let deferred = last.push_delta(&ch.delta, &ch.text, !can_reparse);
                if deferred {
                    self.stream_markdown_dirty = true;
                } else if can_reparse && (!ch.delta.is_empty() || !ch.text.is_empty()) {
                    self.last_stream_reparse = Some(now);
                }
            }
            if !ch.thoughts.is_empty() {
                last.thoughts = ch.thoughts.clone();
            } else if !ch.thoughts_delta.is_empty() {
                last.thoughts.push_str(&ch.thoughts_delta);
            }
            if !ch.model.is_empty() {
                last.model_name = ch.model.clone();
            }
            if !ch.request_id.is_empty() {
                last.request_id = ch.request_id.clone();
            }
            if !ch.response_id.is_empty() {
                last.response_id = ch.response_id.clone();
            }
            if !ch.target_device_name.is_empty() {
                last.target_device_name = ch.target_device_name.clone();
            }
            if !ch.status_text.is_empty() {
                last.status_text = ch.status_text.clone();
            }
            if !ch.artifacts.is_empty() {
                last.artifacts = ch.artifacts.clone();
            }
            for act in &ch.tool_activities {
                let exists = last.tool_activities.iter_mut().find(|existing| {
                    (!act.id.is_empty() && existing.id == act.id)
                        || (act.id.is_empty()
                            && existing.name == act.name
                            && existing.explanation == act.explanation)
                });
                if let Some(existing) = exists {
                    *existing = act.clone();
                } else {
                    last.tool_activities.push(act.clone());
                }
            }
        }
    }

    fn view(&self, _window: window::Id) -> Element<'_, Message> {
        let mut root_row = row![].width(Fill).height(Fill);
        if self.sidebar_open {
            let border_subtle = self.palette.border_subtle;
            let accent = self.palette.accent;
            let bg_sidebar = self.palette.bg_sidebar;
            let bg_canvas = self.palette.bg_canvas;
            let active_line = self.sidebar_resizing || self.sidebar_resize_hovered;
            let line_color = if active_line { accent } else { border_subtle };
            let line_width = if active_line { 2.0 } else { 1.0 };
            let right_pad = if active_line { 2.0 } else { 3.0 };

            // 평소에는 1px 구분선으로 보이되 6px 히트 영역을 확보해 드래그로 사이드바 폭을 조절할 수 있게 한다.
            let handle_visual = row![
                container(space::vertical().height(Fill))
                    .width(Length::Fixed(2.0))
                    .style(move |_| container::Style {
                        background: Some(Background::Color(bg_sidebar)),
                        ..container::Style::default()
                    }),
                container(space::vertical().height(Fill))
                    .width(Length::Fixed(line_width))
                    .style(move |_| container::Style {
                        background: Some(Background::Color(line_color)),
                        ..container::Style::default()
                    }),
                container(space::vertical().height(Fill))
                    .width(Length::Fixed(right_pad))
                    .style(move |_| container::Style {
                        background: Some(Background::Color(bg_canvas)),
                        ..container::Style::default()
                    }),
            ]
            .width(Length::Fixed(6.0))
            .height(Fill);

            let resize_handle = mouse_area(handle_visual)
                .on_press(Message::StartSidebarResize)
                .on_double_click(Message::ResetSidebarWidth)
                .on_enter(Message::SidebarResizeHover(true))
                .on_exit(Message::SidebarResizeHover(false))
                .interaction(mouse::Interaction::ResizingHorizontally);

            root_row = root_row.push(self.view_sidebar()).push(resize_handle);
        }
        root_row = root_row.push(self.view_main());

        let bg_canvas = self.palette.bg_canvas;
        let base: Element<'_, Message> = container(root_row)
            .width(Fill)
            .height(Fill)
            .style(move |_| container::Style {
                background: Some(Background::Color(bg_canvas)),
                ..container::Style::default()
            })
            .into();

        if let Some(preview) = &self.canvas_preview {
            stack![base, self.view_canvas_preview_modal(preview)]
                .width(Fill)
                .height(Fill)
                .into()
        } else if self.search_modal_open {
            stack![base, self.view_search_modal()]
                .width(Fill)
                .height(Fill)
                .into()
        } else if self.settings_modal_open {
            stack![base, self.view_settings_modal()]
                .width(Fill)
                .height(Fill)
                .into()
        } else if self.more_menu_open {
            stack![base, self.view_more_popover_overlay()]
                .width(Fill)
                .height(Fill)
                .into()
        } else if self.filter_menu_open {
            stack![base, self.view_filter_popover_overlay()]
                .width(Fill)
                .height(Fill)
                .into()
        } else if self.conv_menu_target.is_some() {
            stack![base, self.view_conv_menu_popover_overlay()]
                .width(Fill)
                .height(Fill)
                .into()
        } else {
            base
        }
    }

    /// 원본 Gemini 데스크톱 앱과 1:1 대응하는 좌측 사이드바.
    fn view_sidebar(&self) -> Element<'_, Message> {
        let pal = &self.palette;
        let bg_sidebar = pal.bg_sidebar;
        let bg_hover = pal.bg_hover;
        let bg_nav_active = pal.bg_nav_active;
        let text_primary = pal.text_primary;
        let text_strong = pal.text_strong;
        let text_secondary = pal.text_secondary;
        let text_muted = pal.text_muted;
        let accent = pal.accent;

        // 1. 최상단 헤더: 4색 스파클 + Gemini | 돋보기(검색) + 사이드바 토글
        let brand = button(
            row![
                sparkle_color(21.0),
                text("Gemini")
                    .size(17.5)
                    .font(font_semibold())
                    .color(text_strong),
            ]
            .spacing(8)
            .align_y(Alignment::Center),
        )
        .on_press(Message::NewChat)
        .padding([4, 6])
        .style(move |_, status| ghost_btn_style(status, bg_hover, text_strong, 10.0));

        let header = row![
            brand,
            space::horizontal(),
            button(container(Icon::Search.view(17.5, text_primary)).center(Fill))
                .on_press(Message::OpenSearchModal)
                .padding(0)
                .width(Length::Fixed(32.0))
                .height(Length::Fixed(32.0))
                .style(move |_, status| ghost_btn_style(status, bg_hover, text_primary, 16.0)),
            button(container(Icon::SidebarToggle.view(17.5, text_primary)).center(Fill))
                .on_press(Message::ToggleSidebar)
                .padding(0)
                .width(Length::Fixed(32.0))
                .height(Length::Fixed(32.0))
                .style(move |_, status| ghost_btn_style(status, bg_hover, text_primary, 16.0)),
        ]
        .spacing(2)
        .align_y(Alignment::Center);

        // 2. 메인 내비게이션 6개 항목 (새 채팅 / 라이브러리 / 이미지 / 예약 / 학생 / 더보기)
        let nav_row = |icon: Icon, label: &'static str, active: bool, msg: Message| {
            button(
                row![
                    icon.view(17.0, if active { text_strong } else { text_primary }),
                    text(label)
                        .size(13.5)
                        .font(if active { font_medium() } else { font_regular() })
                        .color(if active { text_strong } else { text_primary }),
                ]
                .spacing(11)
                .align_y(Alignment::Center),
            )
            .on_press(msg)
            .width(Fill)
            .padding([7, 11])
            .style(move |_, status| {
                let bg = if active {
                    Some(Background::Color(bg_nav_active))
                } else if matches!(status, button::Status::Hovered) {
                    Some(Background::Color(bg_hover))
                } else {
                    None
                };
                button::Style {
                    background: bg,
                    text_color: if active { text_strong } else { text_primary },
                    border: Border {
                        radius: 18.0.into(),
                        ..Border::default()
                    },
                    ..button::Style::default()
                }
            })
        };

        let loc = self.locale;
        let is_new_chat_active =
            self.active_section == ActiveSection::Chat && self.active_conv_id.is_none();

        let nav_col = column![
            nav_row(Icon::NewChat, t(loc, "새 채팅"), is_new_chat_active, Message::NewChat),
            nav_row(
                Icon::Library,
                t(loc, "라이브러리"),
                self.active_section == ActiveSection::Library,
                Message::SwitchSection(ActiveSection::Library),
            ),
            nav_row(
                Icon::Image,
                t(loc, "이미지"),
                self.active_section == ActiveSection::Images,
                Message::SwitchSection(ActiveSection::Images),
            ),
            nav_row(
                Icon::Clock,
                t(loc, "예약"),
                self.active_section == ActiveSection::Schedules,
                Message::SwitchSection(ActiveSection::Schedules),
            ),
            nav_row(
                Icon::School,
                t(loc, "학생"),
                self.active_section == ActiveSection::Student,
                Message::SwitchSection(ActiveSection::Student),
            ),
            nav_row(
                Icon::MoreHoriz,
                t(loc, "더보기"),
                self.more_menu_open
                    || matches!(
                        self.active_section,
                        ActiveSection::Personalize
                            | ActiveSection::Videos
                            | ActiveSection::GemsHub
                            | ActiveSection::Gem(_)
                    ),
                Message::ToggleMoreMenu,
            ),
        ]
        .spacing(2);

        // 3. 노트북 섹션
        let mut nb_col = column![
            container(
                text(t(loc, "노트북"))
                    .size(11.5)
                    .font(font_medium())
                    .color(text_muted),
            )
            .padding(Padding {
                top: 10.0,
                right: 10.0,
                bottom: 4.0,
                left: 10.0,
            }),
            nav_row(Icon::Plus, t(loc, "새 노트북"), false, Message::NewNotebook),
        ]
        .spacing(1);

        for nb in self.notebooks.iter().take(3) {
            let nb_id = nb.id.clone();
            let active = self.active_section == ActiveSection::Notebooks
                && self.editing_notebook.id == nb.id;
            let title = truncate_single_line(
                t(loc, &nb.title),
                sidebar_text_units(self.sidebar_width, 24),
            );
            nb_col = nb_col.push(
                button(
                    row![
                        Icon::Notebook.view(15.0, text_secondary),
                        text(title)
                            .size(13)
                            .color(if active { text_strong } else { text_primary })
                            .wrapping(text::Wrapping::None),
                    ]
                    .spacing(9)
                    .align_y(Alignment::Center),
                )
                .on_press(Message::OpenNotebook(nb_id))
                .width(Fill)
                .padding([6, 11])
                .style(move |_, status| {
                    let bg = if active {
                        Some(Background::Color(bg_nav_active))
                    } else if matches!(status, button::Status::Hovered) {
                        Some(Background::Color(bg_hover))
                    } else {
                        None
                    };
                    button::Style {
                        background: bg,
                        text_color: text_primary,
                        border: Border {
                            radius: 16.0.into(),
                            ..Border::default()
                        },
                        ..button::Style::default()
                    }
                }),
            );
        }

        nb_col = nb_col.push(nav_row(
            Icon::MoreHoriz,
            t(loc, "모든 노트북"),
            self.active_section == ActiveSection::Notebooks,
            Message::SwitchSection(ActiveSection::Notebooks),
        ));

        // 4. 전체 대화 목록 섹션 (헤더 + 필터 팝오버 + 고정 상단/일반·태스크 병합 + 안읽음 파란 점)
        let filter_active = self.conv_filter != ConvFilter::All;
        let conv_header = container(
            row![
                text(t(loc, self.conv_filter.label()))
                    .size(11.5)
                    .font(font_medium())
                    .color(if filter_active { accent } else { text_muted }),
                space::horizontal(),
                button(
                    container(
                        Icon::Filter.view(
                            16.0,
                            if filter_active || self.filter_menu_open {
                                accent
                            } else {
                                text_primary
                            },
                        )
                    )
                    .center(Fill),
                )
                .on_press(Message::ToggleFilterMenu)
                .padding(0)
                .width(Length::Fixed(26.0))
                .height(Length::Fixed(26.0))
                .style(move |_, status| ghost_btn_style(status, bg_hover, text_primary, 13.0)),
            ]
            .align_y(Alignment::Center),
        )
        .padding(Padding {
            top: 12.0,
            right: 6.0,
            bottom: 4.0,
            left: 10.0,
        });

        let filtered: Vec<&ConversationSummary> = self
            .conversations
            .iter()
            .filter(|c| match self.conv_filter {
                ConvFilter::All => true,
                ConvFilter::Ready => !c.has_unread,
                ConvFilter::NeedsAttention => c.has_unread,
                ConvFilter::InProgress => {
                    self.is_streaming
                        && self.active_conv_id.as_deref() == Some(c.conversation_id.as_str())
                }
                ConvFilter::PinnedOnly => c.pinned,
            })
            .collect();

        let mut conv_col = column![].spacing(1).padding(Padding {
            top: 0.0,
            right: 6.0,
            bottom: 12.0,
            left: 0.0,
        });
        if filtered.is_empty() {
            conv_col = conv_col.push(
                container(
                    text(t(loc, "표시할 대화가 없습니다"))
                        .size(12)
                        .color(text_muted),
                )
                .padding([12, 10]),
            );
        } else {
            for c in filtered {
                conv_col = conv_col.push(self.view_conv_item(c));
            }
            if self.conversations_next_page_token.is_some() {
                let load_lbl = if self.loading_more_conversations {
                    t(loc, "불러오는 중...")
                } else {
                    t(loc, "이전 대화 더 불러오기")
                };
                conv_col = conv_col.push(
                    container(
                        button(
                            text(load_lbl)
                                .size(12)
                                .font(font_medium())
                                .color(text_secondary),
                        )
                        .on_press(Message::LoadMoreConversations)
                        .width(Fill)
                        .padding([7, 10])
                        .style(move |_, status| {
                            ghost_btn_style(status, bg_hover, text_secondary, 12.0)
                        }),
                    )
                    .padding(Padding {
                        top: 6.0,
                        right: 0.0,
                        bottom: 4.0,
                        left: 0.0,
                    }),
                );
            }
        }

        // 내비게이션 + 노트북 + 전체 목록을 하나의 자연스러운 스크롤 컨테이너로 구성 (원본과 동일)
        let scroll_content = column![nav_col, nb_col, conv_header, conv_col]
            .spacing(2)
            .width(Fill);

        let body_scroll = scrollable(scroll_content)
            .height(Fill)
            .width(Fill)
            .direction(scrollable::Direction::Vertical(
                scrollable::Scrollbar::default().width(4).scroller_width(4),
            ))
            .style(move |_, status| AppTheme::subtle_scrollable_style_with_fg(text_primary, status));

        // 5. 하단 프로필 바 (원형 아바타 + 사용자 이름 + 플랜/상태 + 설정 톱니바퀴)
        let avatar_el: Element<'_, Message> = if let Some(handle) = &self.avatar_handle {
            stack![
                image(handle.clone())
                    .width(Length::Fixed(32.0))
                    .height(Length::Fixed(32.0))
                    .content_fit(ContentFit::Cover),
                canvas(AvatarCircleMask {
                    cache: &self.avatar_mask_cache,
                    bg: bg_sidebar,
                })
                    .width(Length::Fixed(32.0))
                    .height(Length::Fixed(32.0)),
            ]
            .width(Length::Fixed(32.0))
            .height(Length::Fixed(32.0))
            .into()
        } else {
            let initial = self
                .profile
                .name
                .chars()
                .next()
                .unwrap_or('G')
                .to_string();
            let accent_fg = pal.accent_fg;
            container(
                text(initial)
                    .size(13.5)
                    .font(font_semibold())
                    .color(accent_fg),
            )
            .width(Length::Fixed(32.0))
            .height(Length::Fixed(32.0))
            .align_x(Alignment::Center)
            .align_y(Alignment::Center)
            .style(move |_| container::Style {
                background: Some(Background::Color(accent)),
                border: Border {
                    radius: 16.0.into(),
                    ..Border::default()
                },
                ..container::Style::default()
            })
            .into()
        };

        let display_name = if !self.profile.name.is_empty() {
            self.profile.name.as_str()
        } else if self.daemon_connected {
            self.daemon_label.as_str()
        } else {
            t(loc, "Gemini 사용자")
        };
        let tier_label = if !self.profile.tier.is_empty() {
            self.profile.tier.as_str()
        } else if self.daemon_connected {
            t(loc, "연결됨")
        } else {
            t(loc, "오프라인")
        };
        let (remote_dot_color, remote_status_label) = if !self.remote_status.current_tool.is_empty()
        {
            let tool = truncate_single_line(&self.remote_status.current_tool, 10);
            (
                accent,
                if loc.is_en() {
                    format!("Running: {tool}")
                } else {
                    format!("실행 중: {tool}")
                },
            )
        } else if self.remote_status.remote_enabled && self.remote_status.punctual_connected {
            (pal.success, t(loc, "원격 연결됨").to_string())
        } else if self.remote_status.remote_enabled {
            (text_muted, t(loc, "원격 대기").to_string())
        } else if self.daemon_connected {
            (text_muted, t(loc, "원격 꺼짐").to_string())
        } else {
            (pal.danger, t(loc, "데몬 끊김").to_string())
        };
        let remote_dot = container(space::horizontal())
            .width(Length::Fixed(7.0))
            .height(Length::Fixed(7.0))
            .style(move |_| container::Style {
                background: Some(Background::Color(remote_dot_color)),
                border: Border {
                    radius: 4.0.into(),
                    ..Border::default()
                },
                ..container::Style::default()
            });

        let profile_trigger = button(
            row![
                avatar_el,
                column![
                    text(truncate_single_line(
                        display_name,
                        sidebar_text_units(self.sidebar_width, 18),
                    ))
                        .size(13.5)
                        .font(font_medium())
                        .color(text_strong)
                        .wrapping(text::Wrapping::None),
                    row![
                        text(tier_label).size(11.0).color(text_secondary),
                        text("·").size(11.0).color(text_muted),
                        remote_dot,
                        text(remote_status_label)
                            .size(11.0)
                            .color(text_secondary)
                            .wrapping(text::Wrapping::None),
                    ]
                    .spacing(4)
                    .align_y(Alignment::Center),
                ]
                .spacing(2),
            ]
            .spacing(10)
            .align_y(Alignment::Center),
        )
        .on_press(Message::OpenSettingsModal)
        .padding([4, 6])
        .width(Fill)
        .style(move |_, status| ghost_btn_style(status, bg_hover, text_primary, 12.0));

        let profile_bar = container(
            row![
                profile_trigger,
                button(container(Icon::Settings.view(18.5, text_primary)).center(Fill))
                    .on_press(Message::OpenSettingsModal)
                    .padding(0)
                    .width(Length::Fixed(34.0))
                    .height(Length::Fixed(34.0))
                    .style(move |_, status| ghost_btn_style(status, bg_hover, text_primary, 17.0)),
            ]
            .spacing(6)
            .align_y(Alignment::Center),
        )
        .padding(Padding {
            top: 8.0,
            right: 4.0,
            bottom: 4.0,
            left: 6.0,
        })
        .width(Fill);

        container(
            column![header, body_scroll, profile_bar]
                .spacing(8)
                .padding(Padding {
                    top: 10.0,
                    right: 10.0,
                    bottom: 8.0,
                    left: 10.0,
                }),
        )
        .width(Length::Fixed(
            (self.sidebar_width - 2.0).max(SIDEBAR_MIN_WIDTH - 2.0),
        ))
        .height(Fill)
        .style(move |_| container::Style {
            background: Some(Background::Color(bg_sidebar)),
            ..container::Style::default()
        })
        .into()
    }

    fn view_conv_item<'a>(&'a self, c: &'a ConversationSummary) -> Element<'a, Message> {
        let pal = &self.palette;
        let is_active = self.active_section == ActiveSection::Chat
            && self.active_conv_id.as_deref() == Some(c.conversation_id.as_str());
        let is_renaming = self.renaming_conv_id.as_deref() == Some(c.conversation_id.as_str());

        let bg_hover = pal.bg_hover;
        let bg_active = pal.bg_active;
        let bg_nav_active = pal.bg_nav_active;
        let bg_composer = pal.bg_composer;
        let text_primary = pal.text_primary;
        let text_strong = pal.text_strong;
        let text_secondary = pal.text_secondary;
        let text_muted = pal.text_muted;
        let accent = pal.accent;
        let danger = pal.danger;

        if is_renaming {
            let input = text_input(t(self.locale, "새 제목"), &self.rename_draft)
                .on_input(Message::RenameDraftChanged)
                .on_submit(Message::SubmitRename)
                .size(12.5)
                .padding([5, 8])
                .style(move |_, _| text_input::Style {
                    background: Background::Color(bg_composer),
                    border: Border {
                        radius: 10.0.into(),
                        width: 1.0,
                        color: accent,
                    },
                    icon: text_secondary,
                    placeholder: text_muted,
                    value: text_primary,
                    selection: bg_active,
                });

            let ok_btn = button(container(Icon::Check.view(13.0, accent)).center(Fill))
                .on_press(Message::SubmitRename)
                .padding(0)
                .width(Length::Fixed(24.0))
                .height(Length::Fixed(24.0))
                .style(move |_, status| ghost_btn_style(status, bg_hover, accent, 8.0));
            let cancel_btn = button(container(Icon::Close.view(12.0, text_secondary)).center(Fill))
                .on_press(Message::CancelRename)
                .padding(0)
                .width(Length::Fixed(24.0))
                .height(Length::Fixed(24.0))
                .style(move |_, status| ghost_btn_style(status, bg_hover, text_secondary, 8.0));

            return row![input, ok_btn, cancel_btn]
                .spacing(3)
                .align_y(Alignment::Center)
                .into();
        }

        let is_menu_target =
            self.conv_menu_target.as_deref() == Some(c.conversation_id.as_str());
        let raw_title = if c.title.is_empty() {
            c.conversation_id.as_str()
        } else {
            c.title.as_str()
        };
        let has_right_badge = c.pinned || c.has_unread;
        let base_units = if is_active || is_menu_target {
            24
        } else if has_right_badge {
            25
        } else {
            28
        };
        let display_title =
            truncate_single_line(raw_title, sidebar_text_units(self.sidebar_width, base_units));

        let mut btn_content = row![
            text(display_title)
                .size(13)
                .font(if is_active || c.has_unread {
                    font_medium()
                } else {
                    font_regular()
                })
                .color(if is_active || c.has_unread {
                    text_strong
                } else {
                    text_primary
                })
                .wrapping(text::Wrapping::None),
            space::horizontal(),
        ]
        .spacing(6)
        .align_y(Alignment::Center);

        if c.pinned {
            btn_content = btn_content.push(Icon::Pin.view(13.0, text_secondary));
        } else if c.has_unread {
            let unread_dot = container(space::horizontal())
                .width(Length::Fixed(6.5))
                .height(Length::Fixed(6.5))
                .style(move |_| container::Style {
                    background: Some(Background::Color(accent)),
                    border: Border {
                        radius: 3.25.into(),
                        ..Border::default()
                    },
                    ..container::Style::default()
                });
            btn_content = btn_content.push(unread_dot);
        }

        let select_btn = button(btn_content)
            .on_press(Message::SelectConversation(
                c.conversation_id.clone(),
                c.title.clone(),
            ))
            .width(Fill)
            .padding([6, 10])
            .style(move |_, status| {
                let bg = if is_active || is_menu_target {
                    Some(Background::Color(bg_nav_active))
                } else if matches!(status, button::Status::Hovered) {
                    Some(Background::Color(bg_hover))
                } else {
                    None
                };
                button::Style {
                    background: bg,
                    text_color: if is_active { text_strong } else { text_primary },
                    border: Border {
                        radius: 16.0.into(),
                        ..Border::default()
                    },
                    ..button::Style::default()
                }
            });

        let more_icon_color = if is_active || is_menu_target {
            text_primary
        } else {
            text_muted
        };
        let more_btn = button(container(Icon::MoreVert.view(15.0, more_icon_color)).center(Fill))
            .on_press(Message::OpenConvMenu(c.conversation_id.clone()))
            .padding(0)
            .width(Length::Fixed(24.0))
            .height(Length::Fixed(24.0))
            .style(move |_, status| ghost_btn_style(status, bg_hover, more_icon_color, 12.0));

        let _ = danger;
        row![select_btn, more_btn]
            .spacing(1)
            .align_y(Alignment::Center)
            .into()
    }

    fn view_main(&self) -> Element<'_, Message> {
        let loc = self.locale;
        let pal = &self.palette;
        let bg_hover = pal.bg_hover;
        let bg_chip = pal.bg_chip;
        let bg_composer = pal.bg_composer;
        let border_subtle = pal.border_subtle;
        let text_primary = pal.text_primary;
        let text_strong = pal.text_strong;
        let text_secondary = pal.text_secondary;
        let accent = pal.accent;
        let accent_soft = pal.accent_soft;
        let warning = pal.warning;
        let danger = pal.danger;

        // 원본과 동일한 극도로 미니멀한 메인 상단 바:
        // 사이드바가 열려 있을 때는 우측 상단에 임시 채팅 아이콘 버튼만 배치한다.
        let mut header_row = row![].spacing(8).align_y(Alignment::Center);

        if !self.sidebar_open {
            header_row = header_row
                .push(
                    button(container(Icon::SidebarToggle.view(17.0, text_secondary)).center(Fill))
                        .on_press(Message::ToggleSidebar)
                        .width(Length::Fixed(32.0))
                        .height(Length::Fixed(32.0))
                        .style(move |_, status| {
                            ghost_btn_style(status, bg_hover, text_secondary, 16.0)
                        }),
                )
                .push(sparkle_color(18.0))
                .push(
                    text("Gemini")
                        .size(17)
                        .font(font_semibold())
                        .color(text_strong),
                );
        }

        if self.active_section == ActiveSection::Chat
            && !self.active_title.is_empty()
            && !self.messages.is_empty()
        {
            header_row = header_row.push(
                text(truncate_single_line(&self.active_title, 42))
                    .size(14)
                    .font(font_medium())
                    .color(text_secondary),
            );
            if !self.active_model.is_empty() {
                header_row = header_row.push(
                    container(text(&self.active_model).size(11).color(text_secondary))
                        .padding([2, 8])
                        .style(move |_| container::Style {
                            background: Some(Background::Color(bg_chip)),
                            border: Border {
                                radius: 10.0.into(),
                                ..Border::default()
                            },
                            ..container::Style::default()
                        }),
                );
            }
        }

        let temp_active = self.temporary_chat;
        let temp_fg = if temp_active { accent } else { text_secondary };
        let temp_btn: Element<'_, Message> = if temp_active {
            button(
                row![
                    Icon::TempChat.view(16.0, temp_fg),
                    text(t(loc, "임시 채팅")).size(12).font(font_medium()).color(temp_fg),
                ]
                .spacing(6)
                .align_y(Alignment::Center),
            )
            .on_press(Message::ToggleTemporary)
            .padding([5, 11])
            .style(move |_, _| button::Style {
                background: Some(Background::Color(accent_soft)),
                text_color: temp_fg,
                border: Border {
                    radius: 16.0.into(),
                    width: 1.0,
                    color: accent,
                },
                ..button::Style::default()
            })
            .into()
        } else {
            button(container(Icon::TempChat.view(19.0, text_primary)).center(Fill))
                .on_press(Message::ToggleTemporary)
                .width(Length::Fixed(36.0))
                .height(Length::Fixed(36.0))
                .style(move |_, status| ghost_btn_style(status, bg_hover, text_primary, 18.0))
                .into()
        };

        header_row = header_row.push(space::horizontal()).push(temp_btn);
        let top_bar = container(header_row).padding([10, 18]).width(Fill);

        let is_welcome_view = self.active_section == ActiveSection::Chat
            && !self.loading_history
            && self.messages.is_empty();

        // 본문 영역 조립
        let mut body = column![top_bar].width(Fill).height(Fill);

        if self.file_hovering {
            body = body.push(
                container(
                    row![
                        Icon::AttachFile.view(15.0, accent),
                        text(t(loc, "여기에 파일을 놓으면 질문에 첨부됩니다 (이미지·PDF·문서·코드)"))
                            .size(13)
                            .font(font_medium())
                            .color(accent),
                    ]
                    .spacing(8)
                    .align_y(Alignment::Center),
                )
                .padding([9, 18])
                .width(Fill)
                .center_x(Fill)
                .style(move |_| container::Style {
                    background: Some(Background::Color(accent_soft)),
                    border: Border {
                        width: 1.0,
                        color: accent,
                        ..Border::default()
                    },
                    ..container::Style::default()
                }),
            );
        }

        if let Some(err) = &self.error_banner {
            let err_bg = Color::from_rgba(danger.r, danger.g, danger.b, 0.12);
            body = body.push(
                container(
                    row![
                        text(err).size(12.5).color(danger),
                        space::horizontal(),
                        button(text(t(loc, "닫기")).size(12))
                            .on_press(Message::DismissError)
                            .padding([4, 10])
                            .style(move |_, status| ghost_btn_style(status, bg_hover, danger, 8.0)),
                    ]
                    .align_y(Alignment::Center)
                    .spacing(12),
                )
                .padding([8, 18])
                .width(Fill)
                .style(move |_| container::Style {
                    background: Some(Background::Color(err_bg)),
                    ..container::Style::default()
                }),
            );
        }

        if self.temporary_chat && !self.messages.is_empty() {
            let warn_bg = Color::from_rgba(warning.r, warning.g, warning.b, 0.10);
            body = body.push(
                container(
                    text(t(
                        loc,
                        "임시 채팅 · 이 대화는 최근 항목에 표시되지 않으며 저장되지 않습니다.",
                    ))
                        .size(12)
                        .color(text_secondary),
                )
                .padding([6, 18])
                .width(Fill)
                .center_x(Fill)
                .style(move |_| container::Style {
                    background: Some(Background::Color(warn_bg)),
                    ..container::Style::default()
                }),
            );
        }

        let main_content: Element<'_, Message> = match &self.active_section {
            ActiveSection::Library => self.view_library(),
            ActiveSection::Images => self.view_images(),
            ActiveSection::Schedules => self.view_schedules(),
            ActiveSection::Student => self.view_student(),
            ActiveSection::Notebooks => self.view_notebooks(),
            ActiveSection::GemsHub => self.view_gems_hub(),
            ActiveSection::Videos => self.view_videos(),
            ActiveSection::Personalize => self.view_personalize(),
            ActiveSection::Gem(bot_id) => self.view_gem_landing(bot_id),
            ActiveSection::Chat => {
                if self.loading_history {
                    container(
                        text(t(loc, "대화 이력을 불러오는 중..."))
                            .size(14)
                            .color(text_secondary),
                    )
                    .center(Fill)
                    .into()
                } else if self.messages.is_empty() {
                    self.view_welcome()
                } else {
                    self.view_chat_messages()
                }
            }
        };

        body = body.push(main_content);

        if let Some((toast_msg, _)) = &self.toast {
            let toast_bar = container(
                container(text(toast_msg).size(12).color(text_primary))
                    .padding([7, 14])
                    .style(move |_| container::Style {
                        background: Some(Background::Color(bg_composer)),
                        border: Border {
                            radius: 16.0.into(),
                            width: 1.0,
                            color: border_subtle,
                        },
                        shadow: Shadow {
                            color: Color::from_rgba(0.0, 0.0, 0.0, 0.14),
                            offset: Vector::new(0.0, 2.0),
                            blur_radius: 10.0,
                        },
                        ..container::Style::default()
                    }),
            )
            .padding(Padding {
                top: 0.0,
                right: 0.0,
                bottom: 10.0,
                left: 0.0,
            })
            .center_x(Fill)
            .width(Fill);
            body = body.push(toast_bar);
        }

        // 환영 화면에서는 채팅(블루) / 태스크(라벤더) 모드별 부드러운 라디얼 글로우 적용
        let layered: Element<'_, Message> = if is_welcome_view {
            let glow_color = if self.composer_mode == ComposerMode::Task {
                pal.glow_task_color
            } else {
                pal.glow_color
            };
            let glow_bg = canvas(WelcomeGlow {
                cache: &self.welcome_glow_cache,
                bg: pal.bg_canvas,
                color: glow_color,
                is_dark: pal.is_dark,
            })
            .width(Fill)
            .height(Fill);
            stack![glow_bg, body].width(Fill).height(Fill).into()
        } else {
            body.into()
        };

        if self.tools_menu_open {
            stack![layered, self.view_tools_popover_overlay(is_welcome_view)]
                .width(Fill)
                .height(Fill)
                .into()
        } else if self.model_menu_open {
            stack![layered, self.view_model_popover_overlay(is_welcome_view)]
                .width(Fill)
                .height(Fill)
                .into()
        } else if self.device_menu_open {
            stack![layered, self.view_device_popover_overlay(is_welcome_view)]
                .width(Fill)
                .height(Fill)
                .into()
        } else if self.mention_query.is_some() {
            stack![layered, self.view_mention_popover_overlay(is_welcome_view)]
                .width(Fill)
                .height(Fill)
                .into()
        } else {
            layered
        }
    }

    /// 원본과 1:1 동일한 환영 화면: 중앙 제목 + 컴팩트 화이트 입력창 + [채팅 | 태스크] 토글.
    fn view_welcome(&self) -> Element<'_, Message> {
        let loc = self.locale;
        let pal = &self.palette;
        let text_strong = pal.text_strong;
        let text_secondary = pal.text_secondary;
        let bg_segment = pal.bg_segment;
        let bg_composer = pal.bg_composer;
        let accent = pal.accent;

        let heading: Element<'_, Message> = if self.temporary_chat {
            column![
                row![
                    Icon::TempChat.view(24.0, accent),
                    text(t(loc, "임시 채팅"))
                        .size(26)
                        .font(font_semibold())
                        .color(text_strong),
                ]
                .spacing(9)
                .align_y(Alignment::Center),
                text(t(loc, "이 대화는 최근 항목에 표시되지 않으며 저장되지 않습니다."))
                    .size(13.5)
                    .color(text_secondary),
            ]
            .spacing(8)
            .align_x(Alignment::Center)
            .into()
        } else if let Some(bot_id) = &self.active_bot_id
            && let Some(bot) = self.find_bot(bot_id)
        {
            column![
                row![
                    Icon::Gem.view(22.0, accent),
                    text(&bot.name)
                        .size(26)
                        .font(font_semibold())
                        .color(text_strong),
                ]
                .spacing(8)
                .align_y(Alignment::Center),
                text(&bot.description)
                    .size(13.5)
                    .color(text_secondary),
            ]
            .spacing(8)
            .align_x(Alignment::Center)
            .into()
        } else if self.composer_mode == ComposerMode::Task {
            text(t(loc, "목록에서 어떤 일을 처리해 드릴까요?"))
                .size(28)
                .font(font_regular())
                .color(text_strong)
                .into()
        } else {
            text(t(loc, "무엇을 도와드릴까요?"))
                .size(30)
                .font(font_regular())
                .color(text_strong)
                .into()
        };

        // 입력창 바로 아래 [ 채팅 | 태스크 ] 세그먼트 토글 (원본 앱 핵심 UI)
        let is_chat = self.composer_mode == ComposerMode::Chat;
        let seg_btn = |label: &'static str, active: bool, mode: ComposerMode| {
            button(
                text(label)
                    .size(13)
                    .font(if active { font_medium() } else { font_regular() })
                    .color(if active { text_strong } else { text_secondary }),
            )
            .on_press(Message::SetComposerMode(mode))
            .padding([6, 16])
            .style(move |_, status| {
                let hovered = matches!(status, button::Status::Hovered);
                button::Style {
                    background: if active {
                        Some(Background::Color(bg_composer))
                    } else if hovered {
                        Some(Background::Color(Color::from_rgba(1.0, 1.0, 1.0, 0.28)))
                    } else {
                        None
                    },
                    text_color: if active { text_strong } else { text_secondary },
                    border: Border {
                        radius: 16.0.into(),
                        ..Border::default()
                    },
                    shadow: if active {
                        Shadow {
                            color: Color::from_rgba(0.0, 0.0, 0.0, 0.08),
                            offset: Vector::new(0.0, 1.0),
                            blur_radius: 4.0,
                        }
                    } else {
                        Shadow::default()
                    },
                    ..button::Style::default()
                }
            })
        };

        let mode_toggle = container(
            row![
                seg_btn(t(loc, "채팅"), is_chat, ComposerMode::Chat),
                seg_btn(t(loc, "태스크"), !is_chat, ComposerMode::Task),
            ]
            .spacing(2)
            .align_y(Alignment::Center),
        )
        .padding(3)
        .style(move |_| container::Style {
            background: Some(Background::Color(bg_segment)),
            border: Border {
                radius: 20.0.into(),
                ..Border::default()
            },
            ..container::Style::default()
        });

        container(
            column![
                space::vertical().height(Length::FillPortion(4)),
                column![
                    heading,
                    column![self.view_composer(true), mode_toggle]
                        .spacing(14)
                        .align_x(Alignment::Center)
                        .width(Fill),
                ]
                .spacing(24)
                .align_x(Alignment::Center)
                .max_width(WELCOME_MAX_WIDTH)
                .width(Fill),
                space::vertical().height(Length::FillPortion(5)),
            ]
            .padding([0, 28])
            .align_x(Alignment::Center)
            .width(Fill)
            .height(Fill),
        )
        .center_x(Fill)
        .width(Fill)
        .height(Fill)
        .into()
    }

    fn view_chat_messages(&self) -> Element<'_, Message> {
        let pal = &self.palette;
        let text_muted = pal.text_muted;
        let border_subtle = pal.border_subtle;
        let mut items: Vec<Element<'_, Message>> = Vec::with_capacity(self.messages.len() + 4);
        let mut last_header_ts: i64 = 0;

        for (idx, msg) in self.messages.iter().enumerate() {
            let ts = msg.timestamp_unix;
            if ts > 0 {
                let show_header = last_header_ts == 0
                    || (ts != last_header_ts
                        && (msg.is_scheduled_run
                            || (ts - last_header_ts).abs() >= 3600
                            || local_day_number(ts, self.local_tz_offset_secs)
                                != local_day_number(last_header_ts, self.local_tz_offset_secs)));
                if show_header {
                    let label = format_turn_timestamp(ts, self.local_tz_offset_secs, self.locale);
                    let line_left = container(space::horizontal())
                        .width(Fill)
                        .height(Length::Fixed(1.0))
                        .style(move |_| container::Style {
                            background: Some(Background::Color(border_subtle)),
                            ..container::Style::default()
                        });
                    let line_right = container(space::horizontal())
                        .width(Fill)
                        .height(Length::Fixed(1.0))
                        .style(move |_| container::Style {
                            background: Some(Background::Color(border_subtle)),
                            ..container::Style::default()
                        });
                    items.push(
                        row![
                            line_left,
                            text(label).size(11.5).color(text_muted),
                            line_right,
                        ]
                        .spacing(12)
                        .align_y(Alignment::Center)
                        .width(Fill)
                        .into(),
                    );
                    last_header_ts = ts;
                }
            }
            let is_last = idx + 1 == self.messages.len();
            items.push(self.view_message(idx, msg, is_last));
        }

        items.push(space::vertical().height(Length::Fixed(16.0)).into());

        let scroll_fg = self.palette.text_primary;
        let chat_scroll = scrollable(
            container(
                column(items)
                    .spacing(22)
                    .padding(Padding {
                        top: 10.0,
                        right: 28.0,
                        bottom: 12.0,
                        left: 24.0,
                    })
                    .max_width(CONTENT_MAX_WIDTH)
                    .width(Fill),
            )
            .center_x(Fill)
            .width(Fill),
        )
        .id(CHAT_SCROLL_ID)
        .height(Fill)
        .width(Fill)
        .direction(scrollable::Direction::Vertical(
            scrollable::Scrollbar::default().width(5).scroller_width(5),
        ))
        .style(move |_, status| AppTheme::subtle_scrollable_style_with_fg(scroll_fg, status));

        let composer_dock = container(
            column![
                self.view_composer(false),
                text(t(
                    self.locale,
                    "Gemini는 실수를 할 수 있으므로 다시 한번 확인하세요.",
                ))
                    .size(11)
                    .color(pal.text_muted),
            ]
            .spacing(7)
            .align_x(Alignment::Center)
            .max_width(CONTENT_MAX_WIDTH)
            .width(Fill),
        )
        .padding(Padding {
            top: 8.0,
            right: 24.0,
            bottom: 12.0,
            left: 24.0,
        })
        .center_x(Fill)
        .width(Fill);

        column![chat_scroll, composer_dock]
            .width(Fill)
            .height(Fill)
            .into()
    }

    /// 원본과 동일한 라운드 카드 입력창: `Gemini에게 물어보기`, 좌측 `+`·`화면 공유`, 우측 `모델 ⌄`·`마이크/전송`.
    fn view_composer(&self, is_welcome: bool) -> Element<'_, Message> {
        let loc = self.locale;
        let pal = &self.palette;
        let bg_composer = pal.bg_composer;
        let bg_chip = pal.bg_chip;
        let bg_hover = pal.bg_hover;
        let bg_active = pal.bg_active;
        let border_subtle = pal.border_subtle;
        let text_primary = pal.text_primary;
        let text_secondary = pal.text_secondary;
        let text_muted = pal.text_muted;
        let accent = pal.accent;
        let accent_soft = pal.accent_soft;
        let accent_fg = pal.accent_fg;
        let success = pal.success;
        let danger = pal.danger;

        let line_count = self.composer.line_count().clamp(1, 6) as f32;
        let min_h = if is_welcome { 52.0 } else { 42.0 };
        let editor_height = (line_count * 24.0 + 12.0).clamp(min_h, 160.0);

        let placeholder: String = if self.composer_mode == ComposerMode::Task {
            t(loc, "태스크를 설명하세요").to_string()
        } else {
            t(loc, "Gemini에게 물어보기").to_string()
        };

        let editor = text_editor(&self.composer)
            .placeholder(placeholder)
            .on_action(Message::EditorAction)
            .key_binding(|key_press| {
                if matches!(
                    key_press.key.as_ref(),
                    keyboard::Key::Named(keyboard::key::Named::Enter)
                ) && !key_press.modifiers.shift()
                {
                    Some(text_editor::Binding::Custom(Message::SendPrompt))
                } else {
                    text_editor::Binding::from_key_press(key_press)
                }
            })
            .size(14.5)
            .padding(Padding {
                top: 4.0,
                right: 6.0,
                bottom: 2.0,
                left: 6.0,
            })
            .height(Length::Fixed(editor_height))
            .style(move |_, _| text_editor::Style {
                background: Background::Color(Color::TRANSPARENT),
                border: Border::default(),
                placeholder: text_muted,
                value: text_primary,
                selection: bg_active,
            });

        let mut card_col = column![].spacing(6).width(Fill);

        // 첨부 파일 칩 표시
        if !self.attached_files.is_empty() {
            let chips: Vec<Element<'_, Message>> = self
                .attached_files
                .iter()
                .enumerate()
                .map(|(i, p)| {
                    let name = p
                        .file_name()
                        .map(|s| s.to_string_lossy().to_string())
                        .unwrap_or_else(|| p.display().to_string());
                    container(
                        row![
                            Icon::AttachFile.view(12.5, text_secondary),
                            text(truncate_single_line(&name, 22))
                                .size(12)
                                .color(text_primary),
                            button(container(Icon::Close.view(11.5, text_secondary)).center(Fill))
                                .on_press(Message::RemoveAttachment(i))
                                .padding(0)
                                .width(Length::Fixed(18.0))
                                .height(Length::Fixed(18.0))
                                .style(move |_, status| {
                                    ghost_btn_style(status, bg_hover, text_secondary, 9.0)
                                }),
                        ]
                        .spacing(5)
                        .align_y(Alignment::Center),
                    )
                    .padding([4, 9])
                    .style(move |_| container::Style {
                        background: Some(Background::Color(bg_chip)),
                        border: Border {
                            radius: 12.0.into(),
                            width: 1.0,
                            color: border_subtle,
                        },
                        ..container::Style::default()
                    })
                    .into()
                })
                .collect();
            card_col = card_col.push(row(chips).spacing(6));
        }

        card_col = card_col.push(editor);

        // 좌측 하단: + 도구/첨부 팝오버 토글 + [화면 공유] 필 버튼
        let plus_icon = if self.tools_menu_open {
            Icon::Close.view(17.5, text_primary)
        } else {
            Icon::Plus.view(19.0, text_primary)
        };
        let attach_btn = button(container(plus_icon).center(Fill))
            .on_press(Message::ToggleToolsMenu)
            .padding(0)
            .width(Length::Fixed(34.0))
            .height(Length::Fixed(34.0))
            .style(move |_, status| ghost_btn_style(status, bg_hover, text_primary, 17.0));

        let screen_label = if self.screen_capturing {
            t(loc, "캡처 중...")
        } else {
            t(loc, "화면 공유")
        };
        let screen_btn = button(
            row![
                Icon::ScreenShare.view(16.0, text_primary),
                text(screen_label)
                    .size(13)
                    .font(font_medium())
                    .color(text_primary),
            ]
            .spacing(6)
            .align_y(Alignment::Center),
        )
        .on_press(Message::StartScreenCapture)
        .padding([6, 13])
        .style(move |_, status| {
            let hovered = matches!(status, button::Status::Hovered);
            button::Style {
                background: Some(Background::Color(if hovered {
                    bg_hover
                } else {
                    bg_chip
                })),
                text_color: text_primary,
                border: Border {
                    radius: 16.0.into(),
                    width: 1.0,
                    color: border_subtle,
                },
                ..button::Style::default()
            }
        });

        // 우측 하단: 모델 선택기 + 마이크(빈 입력 시) / 전송(입력 시) / 중단(스트리밍 시)
        let model_label = self
            .selected_model()
            .map(|m| m.pill_label().to_string())
            .unwrap_or_else(|| t(loc, "자동 모드").into());
        let chevron = if self.model_menu_open {
            Icon::ChevronUp.view(14.5, text_primary)
        } else {
            Icon::ChevronDown.view(14.5, text_primary)
        };
        let menu_open = self.model_menu_open;
        let model_btn = button(
            row![
                text(model_label)
                    .size(13)
                    .font(font_medium())
                    .color(text_primary),
                chevron,
            ]
            .spacing(5)
            .align_y(Alignment::Center),
        )
        .on_press(Message::ToggleModelMenu)
        .padding([6, 10])
        .style(move |_, status| {
            let hovered = matches!(status, button::Status::Hovered);
            button::Style {
                background: Some(Background::Color(if menu_open || hovered {
                    bg_hover
                } else {
                    Color::TRANSPARENT
                })),
                text_color: text_primary,
                border: Border {
                    radius: 16.0.into(),
                    ..Border::default()
                },
                ..button::Style::default()
            }
        });

        let can_send = self.composer_has_text || !self.attached_files.is_empty();
        let is_recording = self.voice_recorder.is_some();

        let action_btn: Element<'_, Message> = if self.is_streaming {
            button(container(Icon::Stop.view(15.0, Color::WHITE)).center(Fill))
                .on_press(Message::AbortStream)
                .width(Length::Fixed(32.0))
                .height(Length::Fixed(32.0))
                .style(move |_, status| {
                    let hovered = matches!(status, button::Status::Hovered);
                    button::Style {
                        background: Some(Background::Color(if hovered {
                            Color::from_rgba(danger.r, danger.g, danger.b, 0.85)
                        } else {
                            danger
                        })),
                        text_color: Color::WHITE,
                        border: Border {
                            radius: 16.0.into(),
                            ..Border::default()
                        },
                        ..button::Style::default()
                    }
                })
                .into()
        } else if is_recording {
            button(
                row![
                    Icon::MicOff.view(13.0, Color::WHITE),
                    text(if loc.is_en() {
                        format!("{}s", self.recording_seconds)
                    } else {
                        format!("{}초", self.recording_seconds)
                    })
                        .size(12)
                        .font(font_medium())
                        .color(Color::WHITE),
                ]
                .spacing(5)
                .align_y(Alignment::Center),
            )
            .on_press(Message::ToggleVoiceRecording)
            .padding([6, 11])
            .style(move |_, _| button::Style {
                background: Some(Background::Color(danger)),
                text_color: Color::WHITE,
                border: Border {
                    radius: 16.0.into(),
                    ..Border::default()
                },
                ..button::Style::default()
            })
            .into()
        } else if can_send {
            button(container(Icon::ArrowUp.view(16.0, accent_fg)).center(Fill))
                .on_press(Message::SendPrompt)
                .width(Length::Fixed(32.0))
                .height(Length::Fixed(32.0))
                .style(move |_, status| {
                    let hovered = matches!(status, button::Status::Hovered);
                    button::Style {
                        background: Some(Background::Color(if hovered {
                            Color::from_rgba(accent.r, accent.g, accent.b, 0.88)
                        } else {
                            accent
                        })),
                        text_color: accent_fg,
                        border: Border {
                            radius: 16.0.into(),
                            ..Border::default()
                        },
                        ..button::Style::default()
                    }
                })
                .into()
        } else {
            // 입력창이 비어있을 때는 원본과 동일하게 마이크 버튼 표시
            button(container(Icon::Mic.view(18.5, text_primary)).center(Fill))
                .on_press(Message::ToggleVoiceRecording)
                .padding(0)
                .width(Length::Fixed(34.0))
                .height(Length::Fixed(34.0))
                .style(move |_, status| ghost_btn_style(status, bg_hover, text_primary, 17.0))
                .into()
        };

        let mut toolbar = row![attach_btn, screen_btn]
            .spacing(6)
            .align_y(Alignment::Center);

        if let Some(f_label) = feature_mode_label(self.selected_feature_mode) {
            let feature_chip = container(
                row![
                    Icon::Sparkle.view(13.0, accent),
                    text(t(loc, f_label))
                        .size(12.5)
                        .font(font_medium())
                        .color(accent),
                    button(container(Icon::Close.view(11.0, accent)).center(Fill))
                        .on_press(Message::ClearFeatureMode)
                        .padding(0)
                        .width(Length::Fixed(18.0))
                        .height(Length::Fixed(18.0))
                        .style(move |_, status| ghost_btn_style(status, bg_hover, accent, 9.0)),
                ]
                .spacing(5)
                .align_y(Alignment::Center),
            )
            .padding([4, 9])
            .style(move |_| container::Style {
                background: Some(Background::Color(accent_soft)),
                border: Border {
                    radius: 14.0.into(),
                    width: 1.0,
                    color: accent,
                },
                ..container::Style::default()
            });
            toolbar = toolbar.push(feature_chip);
        }

        if !is_welcome {
            let is_task = self.composer_mode == ComposerMode::Task;
            let next_mode = if is_task {
                ComposerMode::Chat
            } else {
                ComposerMode::Task
            };
            let task_toggle_btn = button(
                row![
                    Icon::Sparkle.view(14.0, if is_task { accent } else { text_secondary }),
                    text(t(loc, "태스크"))
                        .size(12.5)
                        .font(font_medium())
                        .color(if is_task { accent } else { text_primary }),
                ]
                .spacing(5)
                .align_y(Alignment::Center),
            )
            .on_press(Message::SetComposerMode(next_mode))
            .padding([6, 11])
            .style(move |_, status| {
                let hovered = matches!(status, button::Status::Hovered);
                button::Style {
                    background: Some(Background::Color(if is_task {
                        accent_soft
                    } else if hovered {
                        bg_hover
                    } else {
                        bg_chip
                    })),
                    text_color: if is_task { accent } else { text_primary },
                    border: Border {
                        radius: 16.0.into(),
                        width: 1.0,
                        color: if is_task { accent } else { border_subtle },
                    },
                    ..button::Style::default()
                }
            });
            toolbar = toolbar.push(task_toggle_btn);
        }

        if self.composer_mode == ComposerMode::Task {
            let selected_dev = self.selected_target_device();
            let (dev_label, dev_online) = if let Some(dev) = &selected_dev {
                let is_local = !self.remote_status.device_id.is_empty()
                    && dev
                        .device_id
                        .eq_ignore_ascii_case(&self.remote_status.device_id);
                let online = if is_local {
                    self.remote_status.remote_enabled && self.remote_status.punctual_connected
                } else {
                    dev.remote_enabled
                };
                (truncate_single_line(&dev.device_name, 16), online)
            } else {
                (t(loc, "원격 PC 선택").to_string(), false)
            };
            let dot_color = if dev_online { success } else { text_muted };
            let dev_dot = container(space::horizontal())
                .width(Length::Fixed(7.0))
                .height(Length::Fixed(7.0))
                .style(move |_| container::Style {
                    background: Some(Background::Color(dot_color)),
                    border: Border {
                        radius: 4.0.into(),
                        ..Border::default()
                    },
                    ..container::Style::default()
                });
            let dev_chevron = if self.device_menu_open {
                Icon::ChevronUp.view(13.5, text_primary)
            } else {
                Icon::ChevronDown.view(13.5, text_primary)
            };
            let dev_open = self.device_menu_open;
            let device_btn = button(
                row![
                    dev_dot,
                    Icon::Laptop.view(14.5, text_primary),
                    text(dev_label)
                        .size(12.5)
                        .font(font_medium())
                        .color(text_primary),
                    dev_chevron,
                ]
                .spacing(6)
                .align_y(Alignment::Center),
            )
            .on_press(Message::ToggleDeviceMenu)
            .padding([6, 11])
            .style(move |_, status| {
                let hovered = matches!(status, button::Status::Hovered);
                button::Style {
                    background: Some(Background::Color(if dev_open || hovered {
                        bg_hover
                    } else {
                        bg_chip
                    })),
                    text_color: text_primary,
                    border: Border {
                        radius: 16.0.into(),
                        width: 1.0,
                        color: if dev_open { accent } else { border_subtle },
                    },
                    ..button::Style::default()
                }
            });
            toolbar = toolbar.push(device_btn);
        }

        toolbar = toolbar
            .push(space::horizontal())
            .push(model_btn)
            .push(action_btn);

        card_col = card_col.push(toolbar);

        let shadow_alpha = if pal.is_dark { 0.34 } else { 0.065 };
        container(card_col)
            .padding(Padding {
                top: 15.0,
                right: 16.0,
                bottom: 12.0,
                left: 18.0,
            })
            .width(Fill)
            .style(move |_| container::Style {
                background: Some(Background::Color(bg_composer)),
                border: Border {
                    radius: 26.0.into(),
                    width: 1.0,
                    color: border_subtle,
                },
                shadow: Shadow {
                    color: Color::from_rgba(0.0, 0.0, 0.0, shadow_alpha),
                    offset: Vector::new(0.0, 4.0),
                    blur_radius: 20.0,
                },
                ..container::Style::default()
            })
            .into()
    }

    pub(crate) fn wrap_section_page<'a>(
        &'a self,
        content: iced::widget::Column<'a, Message>,
    ) -> Element<'a, Message> {
        let scroll_fg = self.palette.text_primary;
        scrollable(
            container(
                content
                    .padding(Padding {
                        top: 8.0,
                        right: 28.0,
                        bottom: 28.0,
                        left: 28.0,
                    })
                    .max_width(CONTENT_MAX_WIDTH)
                    .width(Fill),
            )
            .center_x(Fill)
            .width(Fill),
        )
        .height(Fill)
        .width(Fill)
        .direction(scrollable::Direction::Vertical(
            scrollable::Scrollbar::default().width(5).scroller_width(5),
        ))
        .style(move |_, status| AppTheme::subtle_scrollable_style_with_fg(scroll_fg, status))
        .into()
    }

    /// 검색 모달 (`Ctrl+K` / 사이드바 돋보기): 로컬 실시간 필터 + 서버 전문 검색(`SearchConversationHistory`).
    fn view_search_modal(&self) -> Element<'_, Message> {
        let loc = self.locale;
        let pal = &self.palette;
        let bg_popover = pal.bg_popover;
        let bg_chip = pal.bg_chip;
        let bg_hover = pal.bg_hover;
        let bg_active = pal.bg_active;
        let border_subtle = pal.border_subtle;
        let text_primary = pal.text_primary;
        let text_strong = pal.text_strong;
        let text_secondary = pal.text_secondary;
        let text_muted = pal.text_muted;
        let accent = pal.accent;
        let accent_soft = pal.accent_soft;

        let backdrop = mouse_area(
            container(space::horizontal())
                .width(Fill)
                .height(Fill)
                .style(|_| container::Style {
                    background: Some(Background::Color(Color::from_rgba(0.0, 0.0, 0.0, 0.28))),
                    ..container::Style::default()
                }),
        )
        .on_press(Message::CloseSearchModal);

        let search_in = text_input(
            t(loc, "대화 제목이나 본문 키워드 입력 후 Enter..."),
            &self.search_query,
        )
            .id(SEARCH_INPUT_ID)
            .on_input(Message::SearchQueryChanged)
            .on_submit(Message::SubmitServerSearch)
            .size(14.5)
            .padding([9, 10])
            .style(move |_, _| text_input::Style {
                background: Background::Color(Color::TRANSPARENT),
                border: Border::default(),
                icon: text_secondary,
                placeholder: text_muted,
                value: text_primary,
                selection: bg_active,
            });

        let top_bar = container(
            row![
                Icon::Search.view(17.0, text_secondary),
                search_in,
                button(
                    text(if self.search_loading {
                        t(loc, "검색 중...")
                    } else {
                        t(loc, "본문 검색")
                    })
                    .size(12.5)
                    .font(font_medium())
                    .color(accent),
                )
                .on_press(Message::SubmitServerSearch)
                .padding([6, 11])
                .style(move |_, status| ghost_btn_style(status, accent_soft, accent, 12.0)),
                button(container(Icon::Close.view(15.0, text_secondary)).center(Fill))
                    .on_press(Message::CloseSearchModal)
                    .padding(0)
                    .width(Length::Fixed(28.0))
                    .height(Length::Fixed(28.0))
                    .style(move |_, status| ghost_btn_style(status, bg_hover, text_primary, 14.0)),
            ]
            .spacing(8)
            .align_y(Alignment::Center),
        )
        .padding([8, 12])
        .style(move |_| container::Style {
            background: Some(Background::Color(bg_chip)),
            border: Border {
                radius: 14.0.into(),
                ..Border::default()
            },
            ..container::Style::default()
        });

        let mut results = column![].spacing(4);

        if !self.search_hits.is_empty() {
            results = results.push(
                container(
                    text(t(loc, "서버 전체 본문 검색 결과"))
                        .size(11.5)
                        .font(font_semibold())
                        .color(accent),
                )
                .padding([6, 8]),
            );
            for hit in self.search_hits.iter().take(15) {
                let mut col = column![
                    text(if hit.title.is_empty() {
                        hit.conversation_id.as_str()
                    } else {
                        hit.title.as_str()
                    })
                    .size(13.5)
                    .font(font_medium())
                    .color(text_strong),
                ]
                .spacing(2);
                if !hit.snippet.is_empty() {
                    col = col.push(
                        text(truncate_single_line(&hit.snippet, 72))
                            .size(12)
                            .color(text_secondary),
                    );
                }
                results = results.push(
                    button(col)
                        .on_press(Message::SelectSearchHit(
                            hit.conversation_id.clone(),
                            hit.title.clone(),
                            hit.request_id.clone(),
                        ))
                        .width(Fill)
                        .padding([9, 12])
                        .style(move |_, status| {
                            ghost_btn_style(status, bg_hover, text_primary, 12.0)
                        }),
                );
            }
        }

        let q_lower = self.search_query.trim().to_lowercase();
        let local_matches: Vec<&ConversationSummary> = self
            .conversations
            .iter()
            .filter(|c| {
                q_lower.is_empty()
                    || c.title.to_lowercase().contains(&q_lower)
                    || c.conversation_id.to_lowercase().contains(&q_lower)
            })
            .take(15)
            .collect();

        results = results.push(
            container(
                text(if q_lower.is_empty() {
                    t(loc, "최근 대화")
                } else {
                    t(loc, "로컬 목록 일치 항목")
                })
                .size(11.5)
                .font(font_semibold())
                .color(text_muted),
            )
            .padding([6, 8]),
        );

        for c in local_matches {
            results = results.push(
                button(
                    row![
                        text(truncate_single_line(&c.title, 54))
                            .size(13.5)
                            .color(text_primary),
                        space::horizontal(),
                        if c.pinned {
                            Element::from(Icon::Pin.view(12.0, text_muted))
                        } else if c.is_agent_chat {
                            Element::from(text(t(loc, "태스크")).size(11).color(accent))
                        } else {
                            Element::from(space::horizontal().width(Length::Fixed(1.0)))
                        },
                    ]
                    .align_y(Alignment::Center),
                )
                .on_press(Message::SelectConversation(
                    c.conversation_id.clone(),
                    c.title.clone(),
                ))
                .width(Fill)
                .padding([8, 12])
                .style(move |_, status| ghost_btn_style(status, bg_hover, text_primary, 12.0)),
            );
        }

        let scroll = scrollable(results)
            .height(Length::Fixed(380.0))
            .width(Fill)
            .direction(scrollable::Direction::Vertical(
                scrollable::Scrollbar::default().width(4).scroller_width(4),
            ))
            .style(move |_, status| AppTheme::subtle_scrollable_style_with_fg(text_primary, status));

        let card = container(column![top_bar, scroll].spacing(10))
            .width(Length::Fixed(560.0))
            .padding(14)
            .style(move |_| container::Style {
                background: Some(Background::Color(bg_popover)),
                border: Border {
                    radius: 20.0.into(),
                    width: 1.0,
                    color: border_subtle,
                },
                shadow: Shadow {
                    color: Color::from_rgba(0.0, 0.0, 0.0, 0.24),
                    offset: Vector::new(0.0, 12.0),
                    blur_radius: 36.0,
                },
                ..container::Style::default()
            });

        stack![backdrop, container(card).center(Fill)]
            .width(Fill)
            .height(Fill)
            .into()
    }

    /// Canvas 아티팩트 프리뷰 모달 (마크다운 문서는 리치 렌더링, 코드는 고정폭 프리뷰).
    fn view_canvas_preview_modal<'a>(
        &'a self,
        preview: &'a CanvasPreviewState,
    ) -> Element<'a, Message> {
        let msg_idx = preview.msg_idx;
        let art_idx = preview.art_idx;
        let art = &preview.artifact;
        let is_md_doc = preview.parsed_markdown.is_some();
        let loc = self.locale;
        let pal = &self.palette;
        let bg_popover = pal.bg_popover;
        let bg_canvas = pal.bg_canvas;
        let bg_code = pal.bg_code;
        let bg_chip = pal.bg_chip;
        let bg_hover = pal.bg_hover;
        let border_subtle = pal.border_subtle;
        let text_primary = pal.text_primary;
        let text_strong = pal.text_strong;
        let text_secondary = pal.text_secondary;
        let accent = pal.accent;
        let accent_soft = pal.accent_soft;

        let backdrop = mouse_area(
            container(space::horizontal())
                .width(Fill)
                .height(Fill)
                .style(|_| container::Style {
                    background: Some(Background::Color(Color::from_rgba(0.0, 0.0, 0.0, 0.35))),
                    ..container::Style::default()
                }),
        )
        .on_press(Message::CloseCanvasPreview);

        let title_str = if art.title.trim().is_empty() {
            t(loc, "Canvas 문서")
        } else {
            art.title.trim()
        };

        let mut header = row![
            Icon::Canvas.view(18.0, accent),
            text(title_str)
                .size(16)
                .font(font_semibold())
                .color(text_strong),
        ]
        .spacing(8)
        .align_y(Alignment::Center);

        let badge_label = if !art.language.is_empty() {
            art.language.to_uppercase()
        } else if is_md_doc {
            "MARKDOWN".to_string()
        } else {
            String::new()
        };
        if !badge_label.is_empty() {
            header = header.push(
                container(
                    text(badge_label)
                        .size(11)
                        .font(font_medium())
                        .color(text_secondary),
                )
                .padding([2, 8])
                .style(move |_| container::Style {
                    background: Some(Background::Color(bg_chip)),
                    border: Border {
                        radius: 8.0.into(),
                        ..Border::default()
                    },
                    ..container::Style::default()
                }),
            );
        }

        header = header.push(space::horizontal());

        if is_md_doc {
            let toggle_label = if preview.show_raw {
                t(loc, "문서 보기")
            } else {
                t(loc, "원문 보기")
            };
            header = header.push(
                button(text(toggle_label).size(12).color(text_secondary))
                    .on_press(Message::ToggleCanvasRaw)
                    .padding([5, 10])
                    .style(move |_, status| ghost_btn_style(status, bg_hover, text_primary, 8.0)),
            );
        }

        if !art.content.is_empty() {
            let copy_label = if is_md_doc {
                t(loc, "내용 복사")
            } else {
                t(loc, "코드 복사")
            };
            header = header.push(
                button(
                    row![
                        Icon::Copy.view(13.0, text_secondary),
                        text(copy_label).size(12).color(text_secondary),
                    ]
                    .spacing(5)
                    .align_y(Alignment::Center),
                )
                .on_press(Message::CopyCanvasPreview)
                .padding([5, 10])
                .style(move |_, status| ghost_btn_style(status, bg_hover, text_primary, 8.0)),
            );
        }

        if is_html_artifact(art) && !art.content.is_empty() {
            header = header.push(
                button(
                    row![
                        Icon::Play.view(13.0, accent),
                        text(t(loc, "브라우저에서 실행"))
                            .size(12)
                            .font(font_medium())
                            .color(accent),
                    ]
                    .spacing(5)
                    .align_y(Alignment::Center),
                )
                .on_press(Message::RunCanvasHtml(msg_idx, art_idx))
                .padding([5, 10])
                .style(move |_, status| {
                    let hovered = matches!(status, button::Status::Hovered);
                    button::Style {
                        background: Some(Background::Color(if hovered {
                            bg_hover
                        } else {
                            accent_soft
                        })),
                        text_color: accent,
                        border: Border {
                            radius: 8.0.into(),
                            ..Border::default()
                        },
                        ..button::Style::default()
                    }
                }),
            );
        } else if !art.url.is_empty() {
            header = header.push(
                button(
                    row![
                        Icon::Share.view(13.0, accent),
                        text(t(loc, "웹에서 열기"))
                            .size(12)
                            .font(font_medium())
                            .color(accent),
                    ]
                    .spacing(5)
                    .align_y(Alignment::Center),
                )
                .on_press(Message::OpenExternalUrl(art.url.clone()))
                .padding([5, 10])
                .style(move |_, status| ghost_btn_style(status, bg_hover, accent, 8.0)),
            );
        }

        header = header.push(
            button(container(Icon::Close.view(15.0, text_secondary)).center(Fill))
                .on_press(Message::CloseCanvasPreview)
                .padding(0)
                .width(Length::Fixed(30.0))
                .height(Length::Fixed(30.0))
                .style(move |_, status| ghost_btn_style(status, bg_hover, text_primary, 15.0)),
        );

        let body: Element<'a, Message> = if let (false, Some(parsed)) =
            (preview.show_raw, preview.parsed_markdown.as_ref())
        {
            let rendered = markdown_view::view(
                parsed,
                self.palette.markdown_settings(),
                &self.palette,
                loc,
                markdown_view::RenderContext {
                    message_idx: msg_idx,
                    copy_target: markdown_view::CopyTarget::CanvasPreview,
                    generated_images: &[],
                    artifacts: &[],
                    image_cache: &self.library_image_cache,
                    failed_images: &self.failed_image_keys,
                },
            );
            let scroll_fg = self.palette.text_primary;
            container(
                scrollable(container(rendered).padding([18, 22]).width(Fill))
                    .width(Fill)
                    .height(Fill)
                    .direction(scrollable::Direction::Vertical(
                        scrollable::Scrollbar::default().width(6).scroller_width(6),
                    ))
                    .style(move |_, status| AppTheme::subtle_scrollable_style_with_fg(scroll_fg, status)),
            )
            .width(Fill)
            .height(Fill)
            .style(move |_| container::Style {
                background: Some(Background::Color(bg_canvas)),
                border: Border {
                    radius: 12.0.into(),
                    width: 1.0,
                    color: border_subtle,
                },
                ..container::Style::default()
            })
            .into()
        } else {
            let text_code = self.palette.text_code;
            container(
                scrollable(
                    container(
                        text(&art.content)
                            .size(12.5)
                            .font(iced::Font::MONOSPACE)
                            .line_height(text::LineHeight::Relative(1.52))
                            .color(text_code),
                    )
                    .padding([14, 16]),
                )
                .width(Fill)
                .height(Fill)
                .direction(scrollable::Direction::Both {
                    vertical: scrollable::Scrollbar::default().width(6).scroller_width(6),
                    horizontal: scrollable::Scrollbar::default().width(6).scroller_width(6),
                })
                .style(move |_, status| AppTheme::subtle_scrollable_style_with_fg(text_code, status)),
            )
            .width(Fill)
            .height(Fill)
            .style(move |_| container::Style {
                background: Some(Background::Color(bg_code)),
                border: Border {
                    radius: 12.0.into(),
                    width: 1.0,
                    color: border_subtle,
                },
                ..container::Style::default()
            })
            .into()
        };

        let modal_card = container(column![header, body].spacing(12).width(Fill).height(Fill))
            .padding(18)
            .max_width(880.0)
            .width(Fill)
            .height(Length::Fixed(640.0))
            .style(move |_| container::Style {
                background: Some(Background::Color(bg_popover)),
                border: Border {
                    radius: 18.0.into(),
                    width: 1.0,
                    color: border_subtle,
                },
                shadow: Shadow {
                    color: Color::from_rgba(0.0, 0.0, 0.0, 0.25),
                    offset: Vector::new(0.0, 12.0),
                    blur_radius: 32.0,
                },
                ..container::Style::default()
            });

        let centered = container(modal_card)
            .padding(24)
            .center_x(Fill)
            .center_y(Fill)
            .width(Fill)
            .height(Fill);

        stack![backdrop, centered].width(Fill).height(Fill).into()
    }

    /// 설정 모달 (`⚙`): 계정 프로필 · 이 PC 원격 제어(ON/OFF·허용 폴더·MCP·실행 로그) · 등록 기기 · 단축키.
    fn view_settings_modal(&self) -> Element<'_, Message> {
        let loc = self.locale;
        let pal = &self.palette;
        let bg_popover = pal.bg_popover;
        let bg_composer = pal.bg_composer;
        let bg_chip = pal.bg_chip;
        let bg_hover = pal.bg_hover;
        let bg_thought = pal.bg_thought;
        let border_subtle = pal.border_subtle;
        let text_primary = pal.text_primary;
        let text_strong = pal.text_strong;
        let text_secondary = pal.text_secondary;
        let text_muted = pal.text_muted;
        let accent = pal.accent;
        let accent_soft = pal.accent_soft;
        let accent_fg = pal.accent_fg;
        let success = pal.success;
        let danger = pal.danger;

        let backdrop = mouse_area(
            container(space::horizontal())
                .width(Fill)
                .height(Fill)
                .style(|_| container::Style {
                    background: Some(Background::Color(Color::from_rgba(0.0, 0.0, 0.0, 0.28))),
                    ..container::Style::default()
                }),
        )
        .on_press(Message::CloseSettingsModal);

        let header = row![
            text(t(loc, "설정"))
                .size(18)
                .font(font_semibold())
                .color(text_strong),
            space::horizontal(),
            button(
                row![
                    Icon::Refresh.view(13.5, text_secondary),
                    text(t(loc, "동기화")).size(12).color(text_secondary),
                ]
                .spacing(5)
                .align_y(Alignment::Center),
            )
            .on_press(Message::RefreshAll)
            .padding([5, 10])
            .style(move |_, status| ghost_btn_style(status, bg_hover, text_primary, 10.0)),
            button(container(Icon::Close.view(15.0, text_secondary)).center(Fill))
                .on_press(Message::CloseSettingsModal)
                .padding(0)
                .width(Length::Fixed(30.0))
                .height(Length::Fixed(30.0))
                .style(move |_, status| ghost_btn_style(status, bg_hover, text_primary, 15.0)),
        ]
        .spacing(8)
        .align_y(Alignment::Center);

        let profile_card = container(
            row![
                column![
                    row![
                        text(if self.profile.name.is_empty() {
                            t(loc, "Google 계정")
                        } else {
                            self.profile.name.as_str()
                        })
                        .size(15)
                        .font(font_semibold())
                        .color(text_strong),
                        if !self.profile.tier.is_empty() {
                            Element::from(
                                container(
                                    text(&self.profile.tier)
                                        .size(11)
                                        .font(font_medium())
                                        .color(accent),
                                )
                                .padding([2, 8])
                                .style(move |_| container::Style {
                                    background: Some(Background::Color(accent_soft)),
                                    border: Border {
                                        radius: 10.0.into(),
                                        ..Border::default()
                                    },
                                    ..container::Style::default()
                                }),
                            )
                        } else {
                            Element::from(space::horizontal().width(Length::Fixed(1.0)))
                        },
                    ]
                    .spacing(8)
                    .align_y(Alignment::Center),
                    text(&self.profile.email).size(12.5).color(text_secondary),
                    text(format!(
                        "{}: {} ({})",
                        pick(loc, "테마", "Theme"),
                        self.palette.display_name(loc),
                        self.theme_mode.label_for(loc)
                    ))
                    .size(12)
                    .color(text_muted),
                ]
                .spacing(4),
            ]
            .align_y(Alignment::Center),
        )
        .padding(14)
        .width(Fill)
        .style(move |_| container::Style {
            background: Some(Background::Color(bg_chip)),
            border: Border {
                radius: 14.0.into(),
                ..Border::default()
            },
            ..container::Style::default()
        });

        // 환경설정 카드 (테마 수동/자동 전환 + 언어 + 개인 인텔리전스)
        let theme_seg = |m: ThemeMode| {
            let active = self.theme_mode == m;
            button(
                text(m.label_for(loc))
                    .size(12)
                    .font(if active { font_medium() } else { font_regular() })
                    .color(if active { accent } else { text_primary }),
            )
            .on_press(Message::SetThemeMode(m))
            .padding([5, 11])
            .style(move |_, status| {
                let hovered = matches!(status, button::Status::Hovered);
                button::Style {
                    background: Some(Background::Color(if active {
                        accent_soft
                    } else if hovered {
                        bg_hover
                    } else {
                        bg_composer
                    })),
                    text_color: if active { accent } else { text_primary },
                    border: Border {
                        radius: 10.0.into(),
                        width: 1.0,
                        color: if active { accent } else { border_subtle },
                    },
                    ..button::Style::default()
                }
            })
        };

        let lang_seg = |pref: LanguagePref| {
            let active = self.language_pref == pref;
            button(
                text(pref.label(loc))
                    .size(12)
                    .font(if active { font_medium() } else { font_regular() })
                    .color(if active { accent } else { text_primary }),
            )
            .on_press(Message::SetLanguagePref(pref))
            .padding([5, 11])
            .style(move |_, status| {
                let hovered = matches!(status, button::Status::Hovered);
                button::Style {
                    background: Some(Background::Color(if active {
                        accent_soft
                    } else if hovered {
                        bg_hover
                    } else {
                        bg_composer
                    })),
                    text_color: if active { accent } else { text_primary },
                    border: Border {
                        radius: 10.0.into(),
                        width: 1.0,
                        color: if active { accent } else { border_subtle },
                    },
                    ..button::Style::default()
                }
            })
        };

        let pi_on = self.personal_intelligence;
        let pi_btn = button(
            text(if pi_on { t(loc, "켜짐") } else { t(loc, "꺼짐") })
                .size(12)
                .font(font_medium())
                .color(if pi_on { accent } else { text_secondary }),
        )
        .on_press(Message::TogglePersonalIntelligence)
        .padding([4, 12])
        .style(move |_, status| {
            let hovered = matches!(status, button::Status::Hovered);
            button::Style {
                background: Some(Background::Color(if pi_on {
                    accent_soft
                } else if hovered {
                    bg_hover
                } else {
                    bg_composer
                })),
                text_color: if pi_on { accent } else { text_secondary },
                border: Border {
                    radius: 10.0.into(),
                    width: 1.0,
                    color: if pi_on { accent } else { border_subtle },
                },
                ..button::Style::default()
            }
        });

        let tray_on = self.close_to_tray;
        let tray_btn = button(
            text(if tray_on { t(loc, "켜짐") } else { t(loc, "꺼짐") })
                .size(12)
                .font(font_medium())
                .color(if tray_on { accent } else { text_secondary }),
        )
        .on_press(Message::ToggleCloseToTray)
        .padding([4, 12])
        .style(move |_, status| {
            let hovered = matches!(status, button::Status::Hovered);
            button::Style {
                background: Some(Background::Color(if tray_on {
                    accent_soft
                } else if hovered {
                    bg_hover
                } else {
                    bg_composer
                })),
                text_color: if tray_on { accent } else { text_secondary },
                border: Border {
                    radius: 10.0.into(),
                    width: 1.0,
                    color: if tray_on { accent } else { border_subtle },
                },
                ..button::Style::default()
            }
        });

        let prefs_card = container(
            column![
                row![
                    text(t(loc, "화면 테마"))
                        .size(13)
                        .font(font_medium())
                        .color(text_strong),
                    space::horizontal(),
                    row![
                        theme_seg(ThemeMode::System),
                        theme_seg(ThemeMode::Light),
                        theme_seg(ThemeMode::Dark),
                    ]
                    .spacing(5),
                ]
                .align_y(Alignment::Center),
                row![
                    text(t(loc, "언어"))
                        .size(13)
                        .font(font_medium())
                        .color(text_strong),
                    space::horizontal(),
                    row![
                        lang_seg(LanguagePref::System),
                        lang_seg(LanguagePref::Ko),
                        lang_seg(LanguagePref::En),
                    ]
                    .spacing(5),
                ]
                .align_y(Alignment::Center),
                row![
                    column![
                        text(t(loc, "창 닫기 시 트레이로 최소화"))
                            .size(13)
                            .font(font_medium())
                            .color(text_strong),
                        text(t(
                            loc,
                            "창을 닫아도 시스템 트레이에서 백그라운드로 유지합니다.",
                        ))
                            .size(11.5)
                            .color(text_muted),
                    ]
                    .spacing(2),
                    space::horizontal(),
                    tray_btn,
                ]
                .align_y(Alignment::Center),
                row![
                    column![
                        text(t(loc, "개인 인텔리전스 (Labs)"))
                            .size(13)
                            .font(font_medium())
                            .color(text_strong),
                        text(t(
                            loc,
                            "내 활동과 워크스페이스 컨텍스트를 답변에 자동 반영합니다.",
                        ))
                            .size(11.5)
                            .color(text_muted),
                    ]
                    .spacing(2),
                    space::horizontal(),
                    pi_btn,
                ]
                .align_y(Alignment::Center),
            ]
            .spacing(10),
        )
        .padding(14)
        .width(Fill)
        .style(move |_| container::Style {
            background: Some(Background::Color(bg_chip)),
            border: Border {
                radius: 14.0.into(),
                ..Border::default()
            },
            ..container::Style::default()
        });

        // 사용량 한도 카드 (원본 설정 화면 핵심 섹션)
        let mut usage_col = column![
            row![
                text(t(loc, "사용량 한도"))
                    .size(13)
                    .font(font_semibold())
                    .color(text_strong),
                space::horizontal(),
                text(if self.profile.tier.is_empty() {
                    "Gemini"
                } else {
                    self.profile.tier.as_str()
                })
                .size(11.5)
                .color(accent),
            ]
            .align_y(Alignment::Center),
        ]
        .spacing(8);

        if let Some(u) = &self.usage_info {
            if u.buckets.is_empty() {
                usage_col = usage_col.push(
                    text(t(loc, "현재 표시할 사용량 한도 버킷이 없습니다."))
                        .size(12)
                        .color(text_muted),
                );
            } else {
                for b in &u.buckets {
                    let ratio = b.used_ratio.clamp(0.0, 1.0);
                    let pct = (ratio * 100.0).round() as i32;
                    let fill_portion = ((ratio * 100.0).round() as u16).max(if ratio > 0.0 { 2 } else { 0 });
                    let empty_portion = 100u16.saturating_sub(fill_portion);
                    let reset_label = format_reset_time(b.reset_at_unix, loc);
                    let bar_color = if ratio >= 0.85 { danger } else { accent };
                    let mut bar_row = row![].width(Fill).height(Length::Fixed(6.0));
                    if fill_portion > 0 {
                        bar_row = bar_row.push(
                            container(space::horizontal())
                                .width(Length::FillPortion(fill_portion))
                                .height(Fill)
                                .style(move |_| container::Style {
                                    background: Some(Background::Color(bar_color)),
                                    border: Border {
                                        radius: 3.0.into(),
                                        ..Border::default()
                                    },
                                    ..container::Style::default()
                                }),
                        );
                    }
                    if empty_portion > 0 {
                        bar_row = bar_row.push(
                            container(space::horizontal())
                                .width(Length::FillPortion(empty_portion))
                                .height(Fill),
                        );
                    }
                    let bar_track = container(bar_row)
                        .width(Fill)
                        .height(Length::Fixed(6.0))
                        .style(move |_| container::Style {
                            background: Some(Background::Color(bg_composer)),
                            border: Border {
                                radius: 3.0.into(),
                                ..Border::default()
                            },
                            ..container::Style::default()
                        });
                    usage_col = usage_col.push(
                        column![
                            row![
                                text(&b.label)
                                    .size(12.5)
                                    .font(font_medium())
                                    .color(text_primary),
                                space::horizontal(),
                                text(if loc.is_en() {
                                    format!("{pct}% used")
                                } else {
                                    format!("{pct}% 사용")
                                })
                                    .size(11.5)
                                    .color(text_secondary),
                            ]
                            .align_y(Alignment::Center),
                            bar_track,
                            text(reset_label).size(11).color(text_muted),
                        ]
                        .spacing(4),
                    );
                }
            }
            if !u.notice_text.is_empty() {
                usage_col = usage_col.push(
                    text(&u.notice_text)
                        .size(11.5)
                        .color(text_muted)
                        .wrapping(text::Wrapping::WordOrGlyph),
                );
            }
        } else {
            usage_col = usage_col.push(
                text(t(loc, "사용량 한도 정보를 불러오는 중..."))
                    .size(12)
                    .color(text_muted),
            );
        }

        let usage_card = container(usage_col)
            .padding(14)
            .width(Fill)
            .style(move |_| container::Style {
                background: Some(Background::Color(bg_chip)),
                border: Border {
                    radius: 14.0.into(),
                    ..Border::default()
                },
                ..container::Style::default()
            });

        // 이 PC 원격 제어 카드
        let rs = &self.remote_status;
        let local_name = if !rs.device_name.is_empty() {
            rs.device_name.as_str()
        } else {
            self.daemon_label.as_str()
        };
        let (badge_dot, badge_text) = if !rs.current_tool.is_empty() {
            (
                accent,
                if loc.is_en() {
                    format!("Running: {}", rs.current_tool)
                } else {
                    format!("실행 중: {}", rs.current_tool)
                },
            )
        } else if rs.remote_enabled && rs.punctual_connected {
            (success, t(loc, "원격 연결됨").to_string())
        } else if rs.remote_enabled {
            (text_muted, t(loc, "원격 대기").to_string())
        } else {
            (text_muted, t(loc, "원격 꺼짐").to_string())
        };

        let status_badge = container(
            row![
                container(space::horizontal())
                    .width(Length::Fixed(7.0))
                    .height(Length::Fixed(7.0))
                    .style(move |_| container::Style {
                        background: Some(Background::Color(badge_dot)),
                        border: Border {
                            radius: 4.0.into(),
                            ..Border::default()
                        },
                        ..container::Style::default()
                    }),
                text(badge_text)
                    .size(11.5)
                    .font(font_medium())
                    .color(badge_dot),
            ]
            .spacing(5)
            .align_y(Alignment::Center),
        )
        .padding([3, 9])
        .style(move |_| container::Style {
            background: Some(Background::Color(bg_composer)),
            border: Border {
                radius: 10.0.into(),
                width: 1.0,
                color: border_subtle,
            },
            ..container::Style::default()
        });

        let toggle_label = if self.remote_busy {
            t(loc, "처리 중...")
        } else if rs.remote_enabled {
            t(loc, "원격 제어 끄기")
        } else {
            t(loc, "원격 제어 켜기")
        };
        let is_enabled = rs.remote_enabled;
        let toggle_btn = button(
            text(toggle_label)
                .size(12.5)
                .font(font_medium())
                .color(if is_enabled { text_primary } else { accent_fg }),
        )
        .on_press_maybe((!self.remote_busy).then_some(Message::ToggleRemoteEnabled))
        .padding([6, 12])
        .style(move |_, status| {
            let hovered = matches!(status, button::Status::Hovered);
            button::Style {
                background: Some(Background::Color(if is_enabled {
                    if hovered { bg_hover } else { bg_composer }
                } else if hovered {
                    Color::from_rgba(accent.r, accent.g, accent.b, 0.88)
                } else {
                    accent
                })),
                text_color: if is_enabled { text_primary } else { accent_fg },
                border: Border {
                    radius: 10.0.into(),
                    width: if is_enabled { 1.0 } else { 0.0 },
                    color: border_subtle,
                },
                ..button::Style::default()
            }
        });

        let sync_btn = button(
            row![
                Icon::Refresh.view(13.0, text_primary),
                text(t(loc, "지금 재등록")).size(12.5).color(text_primary),
            ]
            .spacing(5)
            .align_y(Alignment::Center),
        )
        .on_press_maybe((!self.remote_busy).then_some(Message::SyncRemoteRegistration))
        .padding([6, 12])
        .style(move |_, status| {
            let hovered = matches!(status, button::Status::Hovered);
            button::Style {
                background: Some(Background::Color(if hovered {
                    bg_hover
                } else {
                    bg_composer
                })),
                text_color: text_primary,
                border: Border {
                    radius: 10.0.into(),
                    width: 1.0,
                    color: border_subtle,
                },
                ..button::Style::default()
            }
        });

        let remote_summary = if loc.is_en() {
            format!(
                "{} tools · {} active sessions · Punctual {}",
                rs.tool_count,
                rs.active_sessions.len(),
                if rs.punctual_connected {
                    "connected"
                } else {
                    "disconnected"
                }
            )
        } else {
            format!(
                "등록 도구 {}개 · 활성 세션 {}개 · Punctual {}",
                rs.tool_count,
                rs.active_sessions.len(),
                if rs.punctual_connected {
                    "연결됨"
                } else {
                    "미연결"
                }
            )
        };

        let mut local_col = column![
            row![
                Icon::Laptop.view(16.0, text_strong),
                text(local_name)
                    .size(14.5)
                    .font(font_semibold())
                    .color(text_strong),
                space::horizontal(),
                status_badge,
            ]
            .spacing(8)
            .align_y(Alignment::Center),
            text(remote_summary).size(12).color(text_secondary),
            row![toggle_btn, sync_btn]
                .spacing(8)
                .align_y(Alignment::Center),
        ]
        .spacing(8);

        // 허용 폴더 관리
        let mut folders_col = column![
            row![
                text(t(loc, "허용 폴더"))
                    .size(12.5)
                    .font(font_semibold())
                    .color(text_strong),
                space::horizontal(),
                button(
                    row![
                        Icon::Plus.view(13.0, accent),
                        text(t(loc, "폴더 선택"))
                            .size(12)
                            .font(font_medium())
                            .color(accent),
                    ]
                    .spacing(4)
                    .align_y(Alignment::Center),
                )
                .on_press_maybe((!self.remote_busy).then_some(Message::PickRemoteFolder))
                .padding([4, 9])
                .style(move |_, status| {
                    let hovered = matches!(status, button::Status::Hovered);
                    button::Style {
                        background: Some(Background::Color(if hovered {
                            bg_hover
                        } else {
                            accent_soft
                        })),
                        text_color: accent,
                        border: Border {
                            radius: 9.0.into(),
                            ..Border::default()
                        },
                        ..button::Style::default()
                    }
                }),
            ]
            .align_y(Alignment::Center),
        ]
        .spacing(6);

        if rs.connected_folders.is_empty() {
            folders_col = folders_col.push(
                text(t(
                    loc,
                    "등록된 허용 폴더가 없습니다. (도구는 기본 작업 경로 기준으로 동작)",
                ))
                    .size(11.5)
                    .color(text_muted),
            );
        } else {
            for folder in &rs.connected_folders {
                let folder_clone = folder.clone();
                folders_col = folders_col.push(
                    container(
                        row![
                            text(folder)
                                .size(12)
                                .color(text_primary)
                                .width(Fill)
                                .wrapping(text::Wrapping::WordOrGlyph),
                            button(container(Icon::Close.view(12.5, text_primary)).center(Fill))
                                .on_press_maybe(
                                    (!self.remote_busy)
                                        .then_some(Message::RemoveRemoteFolder(folder_clone)),
                                )
                                .padding(0)
                                .width(Length::Fixed(24.0))
                                .height(Length::Fixed(24.0))
                                .style(move |_, status| {
                                    let hovered = matches!(status, button::Status::Hovered);
                                    button::Style {
                                        background: Some(Background::Color(if hovered {
                                            Color::from_rgba(danger.r, danger.g, danger.b, 0.16)
                                        } else {
                                            bg_chip
                                        })),
                                        text_color: if hovered { danger } else { text_primary },
                                        border: Border {
                                            radius: 12.0.into(),
                                            width: 1.0,
                                            color: if hovered { danger } else { border_subtle },
                                        },
                                        ..button::Style::default()
                                    }
                                }),
                        ]
                        .spacing(8)
                        .align_y(Alignment::Center),
                    )
                    .padding([6, 10])
                    .width(Fill)
                    .style(move |_| container::Style {
                        background: Some(Background::Color(bg_composer)),
                        border: Border {
                            radius: 9.0.into(),
                            width: 1.0,
                            color: border_subtle,
                        },
                        ..container::Style::default()
                    }),
                );
            }
        }

        let folder_input = text_input(
            t(loc, "폴더 경로 직접 입력 (예: ~/dev)"),
            &self.folder_input_draft,
        )
            .on_input(Message::FolderDraftChanged)
            .on_submit(Message::AddRemoteFolderManual)
            .size(12.5)
            .padding([6, 10])
            .style(move |_, _| text_input::Style {
                background: Background::Color(bg_composer),
                border: Border {
                    radius: 9.0.into(),
                    width: 1.0,
                    color: border_subtle,
                },
                icon: text_muted,
                placeholder: text_muted,
                value: text_primary,
                selection: accent_soft,
            });
        let add_folder_btn = button(
            text(t(loc, "추가"))
                .size(12)
                .font(font_medium())
                .color(text_primary),
        )
            .on_press_maybe(
                (!self.remote_busy && !self.folder_input_draft.trim().is_empty())
                    .then_some(Message::AddRemoteFolderManual),
            )
            .padding([6, 12])
            .style(move |_, status| {
                let hovered = matches!(status, button::Status::Hovered);
                button::Style {
                    background: Some(Background::Color(if hovered {
                        bg_hover
                    } else {
                        bg_composer
                    })),
                    text_color: text_primary,
                    border: Border {
                        radius: 9.0.into(),
                        width: 1.0,
                        color: border_subtle,
                    },
                    ..button::Style::default()
                }
            });
        folders_col = folders_col.push(
            row![folder_input, add_folder_btn]
                .spacing(6)
                .align_y(Alignment::Center),
        );
        local_col = local_col.push(folders_col);

        // MCP 서버 칩
        if !rs.mcp_servers.is_empty() {
            let mcp_chips: Vec<Element<'_, Message>> = rs
                .mcp_servers
                .iter()
                .map(|srv| {
                    let name = if !srv.display_name.is_empty() {
                        srv.display_name.as_str()
                    } else {
                        srv.server_id.as_str()
                    };
                    let dot_c = if srv.enabled { success } else { text_muted };
                    container(
                        row![
                            container(space::horizontal())
                                .width(Length::Fixed(6.0))
                                .height(Length::Fixed(6.0))
                                .style(move |_| container::Style {
                                    background: Some(Background::Color(dot_c)),
                                    border: Border {
                                        radius: 3.0.into(),
                                        ..Border::default()
                                    },
                                    ..container::Style::default()
                                }),
                            text(name).size(11.5).color(text_primary),
                        ]
                        .spacing(5)
                        .align_y(Alignment::Center),
                    )
                    .padding([3, 9])
                    .style(move |_| container::Style {
                        background: Some(Background::Color(bg_composer)),
                        border: Border {
                            radius: 10.0.into(),
                            width: 1.0,
                            color: border_subtle,
                        },
                        ..container::Style::default()
                    })
                    .into()
                })
                .collect();
            local_col = local_col.push(
                column![
                    text(t(loc, "연동 MCP 서버"))
                        .size(12.5)
                        .font(font_semibold())
                        .color(text_strong),
                    row(mcp_chips).spacing(6),
                ]
                .spacing(5),
            );
        }

        // 최근 원격 도구 실행 로그 (내부 상태 변경 이벤트 제외, 최신 6건)
        let tool_events: Vec<_> = rs
            .recent_events
            .iter()
            .rev()
            .filter(|ev| ev.tool_name != "remote_session" && ev.phase != "status_changed")
            .take(6)
            .collect();
        if !tool_events.is_empty() {
            let mut log_col = column![
                text(t(loc, "최근 원격 도구 실행 기록"))
                    .size(12.5)
                    .font(font_semibold())
                    .color(text_strong),
            ]
            .spacing(4);
            for ev in tool_events {
                let ev_dot = if !ev.error.is_empty() {
                    danger
                } else if ev.phase == "started" {
                    accent
                } else {
                    success
                };
                let mut detail = String::new();
                if !ev.args_brief.is_empty() {
                    detail.push_str(&truncate_single_line(&ev.args_brief, 34));
                }
                if !ev.error.is_empty() {
                    if !detail.is_empty() {
                        detail.push_str(" · ");
                    }
                    detail.push_str(&truncate_single_line(&ev.error, 28));
                } else if ev.duration_ms > 0 {
                    if !detail.is_empty() {
                        detail.push_str(" · ");
                    }
                    detail.push_str(&format!("{}ms", ev.duration_ms));
                }
                log_col = log_col.push(
                    container(
                        row![
                            container(space::horizontal())
                                .width(Length::Fixed(6.0))
                                .height(Length::Fixed(6.0))
                                .style(move |_| container::Style {
                                    background: Some(Background::Color(ev_dot)),
                                    border: Border {
                                        radius: 3.0.into(),
                                        ..Border::default()
                                    },
                                    ..container::Style::default()
                                }),
                            text(&ev.tool_name)
                                .size(12)
                                .font(font_medium())
                                .color(text_strong),
                            text(detail).size(11.5).color(text_secondary),
                        ]
                        .spacing(6)
                        .align_y(Alignment::Center),
                    )
                    .padding([5, 9])
                    .width(Fill)
                    .style(move |_| container::Style {
                        background: Some(Background::Color(bg_thought)),
                        border: Border {
                            radius: 8.0.into(),
                            ..Border::default()
                        },
                        ..container::Style::default()
                    }),
                );
            }
            local_col = local_col.push(log_col);
        }

        let local_remote_card = container(local_col)
            .padding(14)
            .width(Fill)
            .style(move |_| container::Style {
                background: Some(Background::Color(bg_chip)),
                border: Border {
                    radius: 14.0.into(),
                    ..Border::default()
                },
                ..container::Style::default()
            });

        let mut dev_col = column![
            text(t(loc, "이 PC 원격 제어"))
                .size(13)
                .font(font_semibold())
                .color(text_strong),
            local_remote_card,
        ]
        .spacing(8);

        let other_devices: Vec<&RegisteredDevice> = self
            .devices
            .iter()
            .filter(|d| {
                rs.device_id.is_empty() || !d.device_id.eq_ignore_ascii_case(&rs.device_id)
            })
            .collect();
        if !other_devices.is_empty() {
            dev_col = dev_col.push(
                text(t(loc, "다른 등록 기기"))
                    .size(13)
                    .font(font_semibold())
                    .color(text_strong),
            );
            for d in other_devices {
                let status_col = if d.remote_enabled { success } else { text_muted };
                let folders = d
                    .folders
                    .as_ref()
                    .map(|f| f.join(", "))
                    .unwrap_or_default();
                let mut info = column![
                    row![
                        Icon::Laptop.view(14.5, text_secondary),
                        text(&d.device_name)
                            .size(13)
                            .font(font_medium())
                            .color(text_strong),
                        text(if d.remote_enabled {
                            if loc.is_en() {
                                format!("Remote active · {} tools", d.tool_count)
                            } else {
                                format!("원격 활성 · 도구 {}개", d.tool_count)
                            }
                        } else {
                            t(loc, "대기").into()
                        })
                        .size(11.5)
                        .color(status_col),
                    ]
                    .spacing(8)
                    .align_y(Alignment::Center),
                ]
                .spacing(2);
                if !folders.is_empty() {
                    info = info.push(
                        text(format!("{}: {folders}", t(loc, "허용 폴더")))
                            .size(11.5)
                            .color(text_muted),
                    );
                }
                dev_col = dev_col.push(
                    container(info)
                        .padding([10, 12])
                        .width(Fill)
                        .style(move |_| container::Style {
                            background: Some(Background::Color(bg_chip)),
                            border: Border {
                                radius: 12.0.into(),
                                ..Border::default()
                            },
                            ..container::Style::default()
                        }),
                );
            }
        }

        let shortcuts = container(
            column![
                text(t(loc, "단축키"))
                    .size(13)
                    .font(font_semibold())
                    .color(text_strong),
                text(t(
                    loc,
                    "Ctrl+N 새 채팅   ·   Ctrl+K 대화 검색   ·   Ctrl+B 사이드바   ·   Esc 닫기/중단",
                ))
                    .size(12)
                    .color(text_secondary),
            ]
            .spacing(5),
        )
        .padding([10, 12])
        .width(Fill)
        .style(move |_| container::Style {
            background: Some(Background::Color(bg_chip)),
            border: Border {
                radius: 12.0.into(),
                ..Border::default()
            },
            ..container::Style::default()
        });

        let modal_scroll = scrollable(
            column![profile_card, usage_card, prefs_card, dev_col, shortcuts]
                .spacing(14)
                .padding(Padding {
                    top: 0.0,
                    right: 6.0,
                    bottom: 0.0,
                    left: 0.0,
                }),
        )
        .height(Length::Fixed(740.0))
        .width(Fill)
        .direction(scrollable::Direction::Vertical(
            scrollable::Scrollbar::default().width(4).scroller_width(4),
        ))
        .style(move |_, status| AppTheme::subtle_scrollable_style_with_fg(text_primary, status));

        let card = container(column![header, modal_scroll].spacing(14))
            .width(Length::Fixed(580.0))
            .padding(18)
            .style(move |_| container::Style {
                background: Some(Background::Color(bg_popover)),
                border: Border {
                    radius: 20.0.into(),
                    width: 1.0,
                    color: border_subtle,
                },
                shadow: Shadow {
                    color: Color::from_rgba(0.0, 0.0, 0.0, 0.24),
                    offset: Vector::new(0.0, 12.0),
                    blur_radius: 36.0,
                },
                ..container::Style::default()
            });

        stack![backdrop, container(card).center(Fill)]
            .width(Fill)
            .height(Fill)
            .into()
    }

    /// 태스크 실행 대상 원격 PC 선택 플로팅 팝오버.
    fn view_device_popover_overlay(&self, is_welcome_view: bool) -> Element<'_, Message> {
        let loc = self.locale;
        let pal = &self.palette;
        let bg_popover = pal.bg_popover;
        let bg_hover = pal.bg_hover;
        let accent_soft = pal.accent_soft;
        let border_subtle = pal.border_subtle;
        let text_primary = pal.text_primary;
        let text_strong = pal.text_strong;
        let text_secondary = pal.text_secondary;
        let text_muted = pal.text_muted;
        let accent = pal.accent;
        let success = pal.success;

        let active_id = self
            .selected_target_device()
            .map(|d| d.device_id);
        let devices = self.available_remote_devices();

        let mut list = column![
            container(
                text(t(loc, "원격 실행 대상 PC"))
                    .size(12)
                    .font(font_medium())
                    .color(text_secondary),
            )
            .padding([6, 12]),
        ]
        .spacing(2);

        if devices.is_empty() {
            list = list.push(
                container(
                    text(t(loc, "등록된 원격 기기가 없습니다."))
                        .size(13)
                        .color(text_secondary),
                )
                .padding([10, 12]),
            );
        } else {
            for d in devices {
                let is_selected = active_id
                    .as_ref()
                    .is_some_and(|id| id.eq_ignore_ascii_case(&d.device_id));
                let is_local = !self.remote_status.device_id.is_empty()
                    && d.device_id
                        .eq_ignore_ascii_case(&self.remote_status.device_id);
                let is_online = if is_local {
                    self.remote_status.remote_enabled && self.remote_status.punctual_connected
                } else {
                    d.remote_enabled
                };
                let dot_c = if is_online { success } else { text_muted };
                let local_prefix = if is_local {
                    pick(loc, "이 PC · ", "This PC · ")
                } else {
                    ""
                };
                let subtitle = if is_online {
                    if loc.is_en() {
                        format!("{local_prefix}Remote connected · {} tools", d.tool_count)
                    } else {
                        format!("{local_prefix}원격 연결됨 · 도구 {}개", d.tool_count)
                    }
                } else if d.remote_enabled {
                    if loc.is_en() {
                        format!("{local_prefix}Remote idle")
                    } else {
                        format!("{local_prefix}원격 대기 중")
                    }
                } else if loc.is_en() {
                    format!("{local_prefix}Remote off")
                } else {
                    format!("{local_prefix}원격 제어 꺼짐")
                };

                let label_col = column![
                    row![
                        container(space::horizontal())
                            .width(Length::Fixed(7.0))
                            .height(Length::Fixed(7.0))
                            .style(move |_| container::Style {
                                background: Some(Background::Color(dot_c)),
                                border: Border {
                                    radius: 4.0.into(),
                                    ..Border::default()
                                },
                                ..container::Style::default()
                            }),
                        Icon::Laptop.view(14.0, text_primary),
                        text(d.device_name.clone())
                            .size(13.5)
                            .font(font_medium())
                            .color(if is_selected { text_strong } else { text_primary }),
                    ]
                    .spacing(6)
                    .align_y(Alignment::Center),
                    text(subtitle).size(11.5).color(text_secondary),
                ]
                .spacing(2);

                let mut item_row = row![label_col, space::horizontal()]
                    .spacing(12)
                    .align_y(Alignment::Center);
                if is_selected {
                    item_row = item_row.push(Icon::Check.view(15.5, accent));
                }

                let id_clone = d.device_id.clone();
                let item_btn = button(item_row)
                    .on_press(Message::SelectDevice(id_clone))
                    .width(Fill)
                    .padding([9, 12])
                    .style(move |_, status| {
                        let hovered = matches!(status, button::Status::Hovered);
                        let bg = if is_selected {
                            Some(Background::Color(accent_soft))
                        } else if hovered {
                            Some(Background::Color(bg_hover))
                        } else {
                            None
                        };
                        button::Style {
                            background: bg,
                            text_color: text_primary,
                            border: Border {
                                radius: 12.0.into(),
                                ..Border::default()
                            },
                            ..button::Style::default()
                        }
                    });
                list = list.push(item_btn);
            }
        }

        // 하단 원격 설정 바로가기
        list = list.push(
            container(space::horizontal())
                .width(Fill)
                .height(Length::Fixed(1.0))
                .style(move |_| container::Style {
                    background: Some(Background::Color(border_subtle)),
                    ..container::Style::default()
                }),
        );
        list = list.push(
            button(
                row![
                    Icon::Settings.view(14.0, text_secondary),
                    text(t(loc, "원격 PC 및 허용 폴더 설정"))
                        .size(12.5)
                        .color(text_secondary),
                ]
                .spacing(7)
                .align_y(Alignment::Center),
            )
            .on_press(Message::OpenSettingsModal)
            .width(Fill)
            .padding([8, 12])
            .style(move |_, status| ghost_btn_style(status, bg_hover, text_primary, 10.0)),
        );

        let shadow_alpha = if pal.is_dark { 0.45 } else { 0.14 };
        let card = container(list)
            .width(Length::Fixed(310.0))
            .padding(8)
            .style(move |_| container::Style {
                background: Some(Background::Color(bg_popover)),
                border: Border {
                    radius: 18.0.into(),
                    width: 1.0,
                    color: border_subtle,
                },
                shadow: Shadow {
                    color: Color::from_rgba(0.0, 0.0, 0.0, shadow_alpha),
                    offset: Vector::new(0.0, 8.0),
                    blur_radius: 26.0,
                },
                ..container::Style::default()
            });

        let backdrop = mouse_area(container(space::horizontal()).width(Fill).height(Fill))
            .on_press(Message::CloseDeviceMenu);

        let max_w = if is_welcome_view {
            WELCOME_MAX_WIDTH
        } else {
            CONTENT_MAX_WIDTH
        };
        let aligned_row = container(
            container(row![card, space::horizontal()].width(Fill))
                .max_width(max_w)
                .width(Fill),
        )
        .padding(Padding {
            top: 0.0,
            right: 24.0,
            bottom: 0.0,
            left: 36.0,
        })
        .center_x(Fill)
        .width(Fill);

        let positioned: Element<'_, Message> = if is_welcome_view {
            column![
                space::vertical().height(Length::FillPortion(6)),
                aligned_row,
                space::vertical().height(Length::FillPortion(3)),
            ]
            .width(Fill)
            .height(Fill)
            .into()
        } else {
            column![
                space::vertical().height(Fill),
                aligned_row,
                space::vertical().height(Length::Fixed(94.0)),
            ]
            .width(Fill)
            .height(Fill)
            .into()
        };

        stack![backdrop, positioned].width(Fill).height(Fill).into()
    }

    fn view_message<'a>(
        &'a self,
        idx: usize,
        msg: &'a ChatMessage,
        is_last: bool,
    ) -> Element<'a, Message> {
        let loc = self.locale;
        let pal = &self.palette;
        let bg_user = pal.bg_user_bubble;
        let bg_hover = pal.bg_hover;
        let bg_thought = pal.bg_thought;
        let bg_chip = pal.bg_chip;
        let border_subtle = pal.border_subtle;
        let text_primary = pal.text_primary;
        let text_secondary = pal.text_secondary;
        let text_muted = pal.text_muted;

        match msg.role {
            Role::User => {
                let mut col = column![].spacing(6).align_x(Alignment::End);

                if !msg.attachment_items.is_empty() {
                    let mut thumb_items: Vec<Element<'_, Message>> = Vec::new();
                    let mut chip_items: Vec<Element<'_, Message>> = Vec::new();
                    for att in &msg.attachment_items {
                        let key = markdown_view::turn_attachment_cache_key(&att.url);
                        if is_attachment_image(att)
                            && let Some(handle) = self.library_image_cache.get(&key)
                        {
                            let thumb_img = image(handle.clone())
                                .content_fit(ContentFit::Cover)
                                .width(Length::Fixed(88.0))
                                .height(Length::Fixed(88.0));
                            let thumb_card = container(thumb_img)
                                .clip(true)
                                .style(move |_| container::Style {
                                    background: Some(Background::Color(bg_chip)),
                                    border: Border {
                                        radius: 12.0.into(),
                                        width: 1.0,
                                        color: border_subtle,
                                    },
                                    ..container::Style::default()
                                });
                            if !att.url.is_empty() {
                                thumb_items.push(
                                    button(thumb_card)
                                        .on_press(Message::OpenExternalUrl(att.url.clone()))
                                        .padding(0)
                                        .style(move |_, status| {
                                            ghost_btn_style(status, bg_hover, text_primary, 12.0)
                                        })
                                        .into(),
                                );
                            } else {
                                thumb_items.push(thumb_card.into());
                            }
                        } else {
                            let label = if !att.name.is_empty() {
                                att.name.as_str()
                            } else if is_attachment_image(att) {
                                t(loc, "이미지")
                            } else {
                                t(loc, "첨부 파일")
                            };
                            let icon = if is_attachment_image(att) {
                                Icon::Image.view(12.0, text_secondary)
                            } else {
                                Icon::AttachFile.view(12.0, text_secondary)
                            };
                            let chip = container(
                                row![icon, text(label).size(12).color(text_secondary)]
                                    .spacing(5)
                                    .align_y(Alignment::Center),
                            )
                            .padding([4, 10])
                            .style(move |_| container::Style {
                                background: Some(Background::Color(bg_chip)),
                                border: Border {
                                    radius: 12.0.into(),
                                    width: 1.0,
                                    color: border_subtle,
                                },
                                ..container::Style::default()
                            });
                            if !att.url.is_empty() {
                                chip_items.push(
                                    button(chip)
                                        .on_press(Message::OpenExternalUrl(att.url.clone()))
                                        .padding(0)
                                        .style(move |_, status| {
                                            ghost_btn_style(status, bg_hover, text_secondary, 12.0)
                                        })
                                        .into(),
                                );
                            } else {
                                chip_items.push(chip.into());
                            }
                        }
                    }
                    if !thumb_items.is_empty() {
                        for chunk in chunk_elements(thumb_items, 3) {
                            col = col.push(row(chunk).spacing(8));
                        }
                    }
                    if !chip_items.is_empty() {
                        for chunk in chunk_elements(chip_items, 3) {
                            col = col.push(row(chunk).spacing(6));
                        }
                    }
                } else if !msg.attachments.is_empty() {
                    let chips: Vec<Element<'_, Message>> = msg
                        .attachments
                        .iter()
                        .map(|name| {
                            container(
                                row![
                                    Icon::AttachFile.view(12.0, text_secondary),
                                    text(name).size(12).color(text_secondary),
                                ]
                                .spacing(5)
                                .align_y(Alignment::Center),
                            )
                            .padding([4, 10])
                            .style(move |_| container::Style {
                                background: Some(Background::Color(bg_chip)),
                                border: Border {
                                    radius: 12.0.into(),
                                    width: 1.0,
                                    color: border_subtle,
                                },
                                ..container::Style::default()
                            })
                            .into()
                        })
                        .collect();
                    col = col.push(row(chips).spacing(6));
                }

                if !msg.raw_text.is_empty() {
                    let bubble = container(
                        text(&msg.raw_text)
                            .size(15)
                            .line_height(text::LineHeight::Relative(1.58))
                            .color(text_primary)
                            .wrapping(text::Wrapping::WordOrGlyph),
                    )
                    .padding([12, 18])
                    .max_width(580.0)
                    .style(move |_| container::Style {
                        background: Some(Background::Color(bg_user)),
                        border: Border {
                            radius: iced::border::Radius {
                                top_left: 20.0,
                                top_right: 6.0,
                                bottom_right: 20.0,
                                bottom_left: 20.0,
                            },
                            ..Border::default()
                        },
                        ..container::Style::default()
                    });

                    let copy_btn = button(
                        row![
                            Icon::Copy.view(12.0, text_muted),
                            text(t(loc, "복사")).size(11).color(text_muted),
                        ]
                        .spacing(4)
                        .align_y(Alignment::Center),
                    )
                    .on_press(Message::CopyChatItem(idx))
                    .padding([3, 8])
                    .style(move |_, status| ghost_btn_style(status, bg_hover, text_muted, 8.0));

                    col = col.push(bubble).push(copy_btn);
                }
                row![space::horizontal(), col].width(Fill).into()
            }
            Role::Model => {
                let is_streaming_here = self.is_streaming && is_last;
                let accent = pal.accent;
                let accent_soft = pal.accent_soft;
                let success = pal.success;
                let warning = pal.warning;
                let mut content = column![].spacing(10).width(Fill);

                if msg.is_scheduled_run {
                    let sched_text = if msg.scheduled_title.trim().is_empty() {
                        t(loc, "예약 실행됨").to_string()
                    } else {
                        format!(
                            "{} · {}",
                            t(loc, "예약 실행됨"),
                            msg.scheduled_title.trim()
                        )
                    };
                    let sched_banner = container(
                        row![
                            Icon::Clock.view(13.0, accent),
                            text(sched_text)
                                .size(12)
                                .font(font_medium())
                                .color(accent),
                        ]
                        .spacing(6)
                        .align_y(Alignment::Center),
                    )
                    .padding([5, 11])
                    .style(move |_| container::Style {
                        background: Some(Background::Color(accent_soft)),
                        border: Border {
                            radius: 12.0.into(),
                            width: 1.0,
                            color: border_subtle,
                        },
                        ..container::Style::default()
                    });
                    content = content.push(sched_banner);
                }

                let mut header = row![sparkle_color(17.0)]
                    .spacing(8)
                    .align_y(Alignment::Center);

                if !msg.model_name.is_empty() {
                    header = header.push(
                        text(&msg.model_name)
                            .size(12)
                            .font(font_medium())
                            .color(text_secondary),
                    );
                }

                if !msg.target_device_name.is_empty() {
                    header = header.push(
                        container(
                            row![
                                Icon::Laptop.view(12.0, accent),
                                text(&msg.target_device_name)
                                    .size(11.5)
                                    .font(font_medium())
                                    .color(accent),
                            ]
                            .spacing(4)
                            .align_y(Alignment::Center),
                        )
                        .padding([2, 8])
                        .style(move |_| container::Style {
                            background: Some(Background::Color(accent_soft)),
                            border: Border {
                                radius: 10.0.into(),
                                ..Border::default()
                            },
                            ..container::Style::default()
                        }),
                    );
                }

                if !msg.thoughts.is_empty() {
                    let toggle_label = if msg.thoughts_expanded {
                        t(loc, "사고 과정 숨기기")
                    } else if is_streaming_here && msg.raw_text.is_empty() {
                        t(loc, "생각하는 중...")
                    } else {
                        t(loc, "사고 과정 표시")
                    };
                    let toggle_icon = if msg.thoughts_expanded {
                        Icon::ChevronUp.view(14.0, text_secondary)
                    } else {
                        Icon::ChevronDown.view(14.0, text_secondary)
                    };
                    header = header.push(
                        button(
                            row![toggle_icon, text(toggle_label).size(12).color(text_secondary)]
                                .spacing(4)
                                .align_y(Alignment::Center),
                        )
                        .on_press(Message::ToggleMessageThoughts(idx))
                        .padding([4, 10])
                        .style(move |_, status| {
                            let hovered = matches!(status, button::Status::Hovered);
                            button::Style {
                                background: Some(Background::Color(if hovered {
                                    bg_hover
                                } else {
                                    bg_thought
                                })),
                                text_color: text_secondary,
                                border: Border {
                                    radius: 12.0.into(),
                                    width: 1.0,
                                    color: border_subtle,
                                },
                                ..button::Style::default()
                            }
                        }),
                    );
                }
                content = content.push(header);

                // 원격 PC 도구 실행 및 진행 상태 카드 (완료 턴은 기본 1줄 접힘, 클릭 시 펼침)
                let live_tool_running =
                    is_streaming_here && !self.remote_status.current_tool.is_empty();
                let has_tools = live_tool_running
                    || !msg.tool_activities.is_empty()
                    || !msg.status_text.is_empty();
                if has_tools {
                    let show_tools_expanded = live_tool_running || msg.tools_expanded;
                    let summary_label: String = if !msg.tool_activities.is_empty() {
                        if loc.is_en() {
                            let n = msg.tool_activities.len();
                            if n == 1 {
                                "Used 1 tool".into()
                            } else {
                                format!("Used {n} tools")
                            }
                        } else {
                            format!("도구 {}건 사용", msg.tool_activities.len())
                        }
                    } else if !msg.status_text.is_empty() {
                        truncate_single_line(msg.status_text.trim(), 52)
                    } else {
                        t(loc, "도구 활동 표시").to_string()
                    };

                    if !show_tools_expanded {
                        let toggle_row = button(
                            row![
                                Icon::Check.view(13.0, success),
                                text(summary_label)
                                    .size(12.0)
                                    .font(font_medium())
                                    .color(text_secondary),
                                Icon::ChevronDown.view(13.5, text_muted),
                            ]
                            .spacing(6)
                            .align_y(Alignment::Center),
                        )
                        .on_press(Message::ToggleMessageTools(idx))
                        .padding([5, 11])
                        .style(move |_, status| {
                            let hovered = matches!(status, button::Status::Hovered);
                            button::Style {
                                background: Some(Background::Color(if hovered {
                                    bg_hover
                                } else {
                                    bg_thought
                                })),
                                text_color: text_secondary,
                                border: Border {
                                    radius: 11.0.into(),
                                    width: 1.0,
                                    color: border_subtle,
                                },
                                ..button::Style::default()
                            }
                        });
                        content = content.push(toggle_row);
                    } else {
                        let mut activity_col = column![].spacing(6).width(Fill);
                        if !live_tool_running {
                            let collapse_btn = button(
                                row![
                                    Icon::Check.view(13.0, success),
                                    text(summary_label)
                                        .size(12.0)
                                        .font(font_medium())
                                        .color(text_secondary),
                                    space::horizontal(),
                                    text(t(loc, "도구 활동 숨기기")).size(11.5).color(text_muted),
                                    Icon::ChevronUp.view(13.5, text_muted),
                                ]
                                .spacing(6)
                                .align_y(Alignment::Center)
                                .width(Fill),
                            )
                            .on_press(Message::ToggleMessageTools(idx))
                            .padding([2, 4])
                            .width(Fill)
                            .style(move |_, status| {
                                ghost_btn_style(status, bg_hover, text_secondary, 8.0)
                            });
                            activity_col = activity_col.push(collapse_btn);
                        }
                        if live_tool_running {
                            let args_part = if !self.remote_status.current_tool_args.is_empty() {
                                format!(
                                    " ({})",
                                    truncate_single_line(&self.remote_status.current_tool_args, 46)
                                )
                            } else {
                                String::new()
                            };
                            activity_col = activity_col.push(
                                row![
                                    container(space::horizontal())
                                        .width(Length::Fixed(7.0))
                                        .height(Length::Fixed(7.0))
                                        .style(move |_| container::Style {
                                            background: Some(Background::Color(accent)),
                                            border: Border {
                                                radius: 4.0.into(),
                                                ..Border::default()
                                            },
                                            ..container::Style::default()
                                        }),
                                    text(if loc.is_en() {
                                        format!(
                                            "Running: {}{}",
                                            self.remote_status.current_tool, args_part
                                        )
                                    } else {
                                        format!(
                                            "실행 중: {}{}",
                                            self.remote_status.current_tool, args_part
                                        )
                                    })
                                    .size(12.5)
                                    .font(font_medium())
                                    .color(accent),
                                ]
                                .spacing(7)
                                .align_y(Alignment::Center),
                            );
                        }
                        for act in &msg.tool_activities {
                            let title = if !act.name.is_empty() {
                                act.name.as_str()
                            } else {
                                t(loc, "원격 도구 실행")
                            };
                            let detail = if !act.explanation.is_empty() {
                                act.explanation.as_str()
                            } else if !act.summary.is_empty() {
                                act.summary.as_str()
                            } else {
                                act.action.as_str()
                            };
                            let mut act_row = row![
                                Icon::Check.view(13.0, success),
                                text(title)
                                    .size(12.5)
                                    .font(font_medium())
                                    .color(text_primary),
                            ]
                            .spacing(6)
                            .align_y(Alignment::Center);
                            if !detail.is_empty() {
                                act_row = act_row.push(
                                    text(truncate_single_line(detail, 58))
                                        .size(12.0)
                                        .color(text_secondary),
                                );
                            }
                            activity_col = activity_col.push(act_row);
                        }
                        if !msg.status_text.is_empty() {
                            activity_col = activity_col.push(
                                text(msg.status_text.trim())
                                    .size(12.5)
                                    .color(text_secondary)
                                    .wrapping(text::Wrapping::WordOrGlyph),
                            );
                        }
                        let activity_card = container(activity_col)
                            .padding([10, 14])
                            .width(Fill)
                            .style(move |_| container::Style {
                                background: Some(Background::Color(bg_thought)),
                                border: Border {
                                    radius: 12.0.into(),
                                    width: 1.0,
                                    color: border_subtle,
                                },
                                ..container::Style::default()
                            });
                        content = content.push(activity_card);
                    }
                }

                if msg.thoughts_expanded && !msg.thoughts.is_empty() {
                    let thought_box = container(
                        text(&msg.thoughts)
                            .size(13)
                            .line_height(text::LineHeight::Relative(1.58))
                            .color(text_secondary)
                            .wrapping(text::Wrapping::WordOrGlyph),
                    )
                    .padding([12, 16])
                    .width(Fill)
                    .style(move |_| container::Style {
                        background: Some(Background::Color(bg_thought)),
                        border: Border {
                            radius: 12.0.into(),
                            width: 1.0,
                            color: border_subtle,
                        },
                        ..container::Style::default()
                    });
                    content = content.push(thought_box);
                }

                let has_body = !msg.raw_text.is_empty()
                    || !msg.generated_images.is_empty()
                    || !msg.artifacts.is_empty();

                if !has_body && is_streaming_here {
                    let waiting_msg = if live_tool_running || !msg.tool_activities.is_empty() {
                        t(loc, "원격 PC 실행 결과를 정리하는 중...")
                    } else {
                        t(loc, "생각하는 중...")
                    };
                    content = content.push(
                        text(waiting_msg)
                            .size(14)
                            .color(text_secondary),
                    );
                } else if has_body {
                    let md_view = markdown_view::view(
                        &msg.markdown,
                        self.palette.markdown_settings(),
                        &self.palette,
                        loc,
                        markdown_view::RenderContext {
                            message_idx: idx,
                            copy_target: markdown_view::CopyTarget::Message(idx),
                            generated_images: &msg.generated_images,
                            artifacts: &msg.artifacts,
                            image_cache: &self.library_image_cache,
                            failed_images: &self.failed_image_keys,
                        },
                    );
                    content = content.push(md_view);
                }

                if msg.aborted && !is_streaming_here {
                    let aborted_badge = container(
                        row![
                            Icon::Stop.view(12.5, warning),
                            text(t(loc, "응답 생성이 중단되었습니다"))
                                .size(12.0)
                                .font(font_medium())
                                .color(warning),
                        ]
                        .spacing(6)
                        .align_y(Alignment::Center),
                    )
                    .padding([5, 11])
                    .style(move |_| container::Style {
                        background: Some(Background::Color(bg_thought)),
                        border: Border {
                            radius: 10.0.into(),
                            width: 1.0,
                            color: border_subtle,
                        },
                        ..container::Style::default()
                    });
                    content = content.push(aborted_badge);
                }

                if !is_streaming_here && (has_body || msg.aborted) {
                    let fb = self.message_feedback.get(&idx).copied();
                    let up_active = fb == Some(true);
                    let down_active = fb == Some(false);
                    let mut actions = row![
                        button(
                            container(
                                Icon::ThumbsUp
                                    .view(14.5, if up_active { accent } else { text_secondary })
                            )
                            .center(Fill),
                        )
                        .on_press(Message::SendFeedback(idx, true))
                        .width(Length::Fixed(28.0))
                        .height(Length::Fixed(28.0))
                        .style(move |_, status| {
                            ghost_btn_style(
                                status,
                                bg_hover,
                                if up_active { accent } else { text_secondary },
                                14.0,
                            )
                        }),
                        button(
                            container(
                                Icon::ThumbsDown
                                    .view(14.5, if down_active { accent } else { text_secondary })
                            )
                            .center(Fill),
                        )
                        .on_press(Message::SendFeedback(idx, false))
                        .width(Length::Fixed(28.0))
                        .height(Length::Fixed(28.0))
                        .style(move |_, status| {
                            ghost_btn_style(
                                status,
                                bg_hover,
                                if down_active { accent } else { text_secondary },
                                14.0,
                            )
                        }),
                    ]
                    .spacing(4)
                    .align_y(Alignment::Center);

                    if !msg.raw_text.is_empty() {
                        actions = actions.push(
                            button(
                                row![
                                    Icon::Copy.view(13.0, text_secondary),
                                    text(t(loc, "복사")).size(12).color(text_secondary),
                                ]
                                .spacing(5)
                                .align_y(Alignment::Center),
                            )
                            .on_press(Message::CopyChatItem(idx))
                            .padding([4, 10])
                            .style(move |_, status| {
                                ghost_btn_style(status, bg_hover, text_secondary, 8.0)
                            }),
                        );
                    }

                    if let Some(cid) = &self.active_conv_id {
                        actions = actions.push(
                            button(
                                row![
                                    Icon::Share.view(13.0, text_secondary),
                                    text(t(loc, "공유")).size(12).color(text_secondary),
                                ]
                                .spacing(5)
                                .align_y(Alignment::Center),
                            )
                            .on_press(Message::ShareConversation(
                                cid.clone(),
                                self.active_title.clone(),
                            ))
                            .padding([4, 10])
                            .style(move |_, status| {
                                ghost_btn_style(status, bg_hover, text_secondary, 8.0)
                            }),
                        );
                    }

                    if is_last {
                        actions = actions.push(
                            button(
                                row![
                                    Icon::Refresh.view(13.0, text_secondary),
                                    text(t(loc, "다시 생성")).size(12).color(text_secondary),
                                ]
                                .spacing(5)
                                .align_y(Alignment::Center),
                            )
                            .on_press(Message::RegenerateLast)
                            .padding([4, 10])
                            .style(move |_, status| {
                                ghost_btn_style(status, bg_hover, text_secondary, 8.0)
                            }),
                        );
                    }
                    content = content.push(actions);
                }

                container(content).padding(4).width(Fill).into()
            }
        }
    }
}

pub(crate) fn card_style(bg: Color, border_col: Color) -> container::Style {
    container::Style {
        background: Some(Background::Color(bg)),
        border: Border {
            radius: 16.0.into(),
            width: 1.0,
            color: border_subtle_or(border_col),
        },
        ..container::Style::default()
    }
}

fn border_subtle_or(c: Color) -> Color {
    c
}

/// 사이드바 폭을 최소·최대 한계 및 현재 창 폭의 본문 확보 여백에 맞춰 클램프한다.
fn clamp_sidebar_width(desired: f32, window_width: f32) -> f32 {
    let max_w = if window_width > 0.0 {
        (window_width - SIDEBAR_MAIN_MIN_RESERVE).clamp(SIDEBAR_MIN_WIDTH, SIDEBAR_MAX_WIDTH)
    } else {
        SIDEBAR_MAX_WIDTH
    };
    desired.round().clamp(SIDEBAR_MIN_WIDTH, max_w)
}

/// 현재 사이드바 폭에 비례해 단일 줄 말줄임 허용 단위(ASCII=1, 비ASCII=2)를 계산한다.
fn sidebar_text_units(sidebar_width: f32, base_units: usize) -> usize {
    let delta_px = sidebar_width - SIDEBAR_DEFAULT_WIDTH;
    ((base_units as f32) + delta_px / 7.5)
        .round()
        .clamp(12.0, 96.0) as usize
}

/// 사이드바 대화 제목을 단일 줄 표시 폭에 맞게 말줄임(`…`) 처리한다.
pub(crate) fn truncate_single_line(s: &str, max_units: usize) -> String {
    let single = s.replace(['\n', '\r'], " ");
    let mut units = 0usize;
    let mut out = String::new();
    for ch in single.chars() {
        let w = if ch.is_ascii() { 1 } else { 2 };
        if units + w > max_units {
            out.push('…');
            return out;
        }
        out.push(ch);
        units += w;
    }
    out
}

pub(crate) fn ghost_btn_style(
    status: button::Status,
    hover_bg: Color,
    fg: Color,
    radius: f32,
) -> button::Style {
    button::Style {
        background: if matches!(status, button::Status::Hovered) {
            Some(Background::Color(hover_bg))
        } else {
            None
        },
        text_color: fg,
        border: Border {
            radius: radius.into(),
            ..Border::default()
        },
        ..button::Style::default()
    }
}

/// 입력창 끝부분에 `@검색어`가 입력 중이면 해당 검색어를 추출한다.
fn extract_mention_query(text: &str) -> Option<String> {
    let trimmed = text.trim_end_matches('\n');
    let last_token = trimmed.rsplit([' ', '\n', '\t']).next()?;
    let rest = last_token.strip_prefix('@')?;
    if rest.chars().count() <= 24 {
        Some(rest.to_string())
    } else {
        None
    }
}

/// 허용된 외부 링크 스킴(`http://`, `https://`)인지 검증한다.
fn is_safe_external_url(url: &str) -> bool {
    let trimmed = url.trim();
    let lower = trimmed.to_ascii_lowercase();
    (lower.starts_with("https://") || lower.starts_with("http://"))
        && !trimmed.chars().any(|c| c.is_control() || c.is_whitespace())
}

/// 마크다운·버튼에서 호출하는 외부 URL을 스킴 검증 후 `xdg-open`으로 연다.
fn open_safe_external_url(url: &str) -> bool {
    let trimmed = url.trim();
    if !is_safe_external_url(trimmed) {
        return false;
    }
    std::process::Command::new("xdg-open")
        .arg(trimmed)
        .spawn()
        .is_ok()
}

/// 유닉스 초 단위 초기화 시각을 현재 UI 로케일에 맞게 사람이 읽기 쉬운 문구로 변환한다.
fn format_reset_time(reset_unix: i64, loc: Locale) -> String {
    if reset_unix <= 0 {
        return t(loc, "주기적 자동 초기화").to_string();
    }
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let diff = reset_unix - now;
    if diff <= 0 {
        return t(loc, "곧 초기화됨").to_string();
    }
    let hours = diff / 3600;
    let mins = (diff % 3600) / 60;
    if loc.is_en() {
        if hours >= 24 {
            let days = hours / 24;
            let rem_h = hours % 24;
            format!("Resets in about {days}d {rem_h}h")
        } else if hours > 0 {
            format!("Resets in about {hours}h {mins}m")
        } else {
            format!("Resets in about {}m", mins.max(1))
        }
    } else if hours >= 24 {
        let days = hours / 24;
        let rem_h = hours % 24;
        format!("약 {days}일 {rem_h}시간 후 초기화")
    } else if hours > 0 {
        format!("약 {hours}시간 {mins}분 후 초기화")
    } else {
        format!("약 {}분 후 초기화", mins.max(1))
    }
}

/// 첨부 메타데이터가 이미지 형식인지 판별한다.
fn is_attachment_image(att: &TurnAttachment) -> bool {
    if att.mime_type.to_ascii_lowercase().starts_with("image/") {
        return true;
    }
    if att.width > 0 && att.height > 0 {
        return true;
    }
    let lower_name = att.name.to_ascii_lowercase();
    lower_name.ends_with(".png")
        || lower_name.ends_with(".jpg")
        || lower_name.ends_with(".jpeg")
        || lower_name.ends_with(".webp")
        || lower_name.ends_with(".gif")
        || att.url.contains("lh3.googleusercontent.com/")
}

/// Canvas 아티팩트가 브라우저에서 바로 실행 가능한 HTML 문서인지 판별한다.
fn is_html_artifact(art: &TurnArtifact) -> bool {
    let lang = art.language.trim();
    if lang.eq_ignore_ascii_case("html") || lang.eq_ignore_ascii_case("htm") {
        return true;
    }
    let trimmed = art.content.trim_start().as_bytes();
    trimmed
        .get(..14)
        .is_some_and(|h| h.eq_ignore_ascii_case(b"<!doctype html"))
        || trimmed
            .get(..5)
            .is_some_and(|h| h.eq_ignore_ascii_case(b"<html"))
}

/// UI 요소 벡터를 지정한 최대 개수의 행 묶음으로 분할한다.
fn chunk_elements<'a>(items: Vec<Element<'a, Message>>, per_row: usize) -> Vec<Vec<Element<'a, Message>>> {
    let per_row = per_row.max(1);
    let mut rows = Vec::new();
    let mut cur = Vec::with_capacity(per_row);
    for item in items {
        cur.push(item);
        if cur.len() == per_row {
            rows.push(std::mem::replace(&mut cur, Vec::with_capacity(per_row)));
        }
    }
    if !cur.is_empty() {
        rows.push(cur);
    }
    rows
}

/// Canvas HTML 아티팩트를 사용자 전용 임시 디렉터리에 저장하고 기본 브라우저로 연다.
fn write_and_open_canvas_html(title: &str, content: &str) -> Result<PathBuf, String> {
    use std::fs;
    #[cfg(unix)]
    use std::os::unix::fs::PermissionsExt;

    let base_dir = std::env::var_os("XDG_RUNTIME_DIR")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os("XDG_CACHE_HOME")
                .filter(|v| !v.is_empty())
                .map(PathBuf::from)
        })
        .unwrap_or_else(std::env::temp_dir);
    let dir = base_dir.join("gemini-ui-canvas");
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    #[cfg(unix)]
    let _ = fs::set_permissions(&dir, fs::Permissions::from_mode(0o700));

    // 24시간 이상 지난 이전 임시 HTML 파일 자동 정리
    if let Ok(entries) = fs::read_dir(&dir) {
        let now = std::time::SystemTime::now();
        for entry in entries.flatten() {
            let p = entry.path();
            if p.extension().and_then(|e| e.to_str()) == Some("html")
                && let Ok(meta) = entry.metadata()
                && let Ok(modified) = meta.modified()
                && now.duration_since(modified).map(|d| d.as_secs() > 86_400).unwrap_or(false)
            {
                let _ = fs::remove_file(p);
            }
        }
    }

    let slug: String = title
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .take(36)
        .collect();
    let safe_slug = if slug.trim_matches('_').is_empty() {
        "canvas".to_string()
    } else {
        slug
    };
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let path = dir.join(format!("{safe_slug}_{}_{nanos}.html", std::process::id()));
    fs::write(&path, content).map_err(|e| e.to_string())?;
    #[cfg(unix)]
    let _ = fs::set_permissions(&path, fs::Permissions::from_mode(0o600));

    std::process::Command::new("xdg-open")
        .arg(&path)
        .spawn()
        .map_err(|e| e.to_string())?;
    Ok(path)
}

/// 시스템 로컬 타임존 UTC 오프셋(초)을 부팅 시 1회 조회한다.
fn detect_local_tz_offset_secs() -> i64 {
    std::process::Command::new("date")
        .arg("+%z")
        .output()
        .ok()
        .and_then(|out| String::from_utf8(out.stdout).ok())
        .and_then(|s| parse_tz_offset_hhmm(s.trim()))
        .unwrap_or(0)
}

fn parse_tz_offset_hhmm(s: &str) -> Option<i64> {
    if !s.is_ascii() || s.len() != 5 {
        return None;
    }
    let sign = match s.as_bytes()[0] {
        b'+' => 1i64,
        b'-' => -1i64,
        _ => return None,
    };
    let hours: i64 = s[1..3].parse().ok()?;
    let mins: i64 = s[3..5].parse().ok()?;
    if hours > 14 || mins >= 60 {
        return None;
    }
    Some(sign * (hours * 3600 + mins * 60))
}

fn local_day_number(unix_secs: i64, tz_offset_secs: i64) -> i64 {
    (unix_secs + tz_offset_secs).div_euclid(86_400)
}

/// 유닉스 일수(1970-01-01 = 0)를 그레고리력 `(년, 월, 일)`로 변환한다 (Howard Hinnant 알고리즘).
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = (if mp < 10 { mp + 3 } else { mp - 9 }) as u32;
    let year = y + i64::from(m <= 2);
    (year, m, d)
}

/// 대화 턴 타임스탬프를 현재 로케일·로컬 시간대 기준의 날짜/시각 라벨로 변환한다.
fn format_turn_timestamp(unix_secs: i64, tz_offset_secs: i64, loc: Locale) -> String {
    let local = unix_secs + tz_offset_secs;
    let day = local.div_euclid(86_400);
    let sec_of_day = local.rem_euclid(86_400);
    let hour24 = (sec_of_day / 3600) as u32;
    let minute = ((sec_of_day % 3600) / 60) as u32;
    let (year, month, dom) = civil_from_days(day);

    let now_unix = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(unix_secs);
    let today_day = local_day_number(now_unix, tz_offset_secs);
    let (now_year, _, _) = civil_from_days(today_day);

    if loc.is_en() {
        const MONTHS: [&str; 12] = [
            "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
        ];
        let (ampm, h12) = if hour24 == 0 {
            ("AM", 12)
        } else if hour24 < 12 {
            ("AM", hour24)
        } else if hour24 == 12 {
            ("PM", 12)
        } else {
            ("PM", hour24 - 12)
        };
        let date_part = if day == today_day {
            "Today".to_string()
        } else if day + 1 == today_day {
            "Yesterday".to_string()
        } else {
            let mon = MONTHS[(month.saturating_sub(1) as usize).min(11)];
            if year == now_year {
                format!("{mon} {dom}")
            } else {
                format!("{mon} {dom}, {year}")
            }
        };
        format!("{date_part} · {h12}:{minute:02} {ampm}")
    } else {
        let (ampm, h12) = if hour24 == 0 {
            ("오전", 12)
        } else if hour24 < 12 {
            ("오전", hour24)
        } else if hour24 == 12 {
            ("오후", 12)
        } else {
            ("오후", hour24 - 12)
        };
        let date_part = if day == today_day {
            "오늘".to_string()
        } else if day + 1 == today_day {
            "어제".to_string()
        } else if year == now_year {
            format!("{month}월 {dom}일")
        } else {
            format!("{year}년 {month}월 {dom}일")
        };
        format!("{date_part} · {ampm} {h12}:{minute:02}")
    }
}

/// Omarchy 테마 변경 파일(generation)을 3초 간격으로 감시해 실제 변경 시에만 이벤트를 방출한다.
fn watch_omarchy_theme() -> impl iced::futures::Stream<Item = String> {
    iced::stream::channel(
        4,
        |mut tx: iced::futures::channel::mpsc::Sender<String>| async move {
            let mut last = AppTheme::current_omarchy_generation();
            loop {
                tokio::time::sleep(Duration::from_secs(3)).await;
                let cur = AppTheme::current_omarchy_generation();
                if cur != last {
                    last = cur.clone();
                    if let Some(generation) = cur
                        && let Err(err) = tx.try_send(generation)
                        && err.is_disconnected()
                    {
                        break;
                    }
                }
            }
        },
    )
}

#[cfg(test)]
mod url_safety_tests {
    use super::{
        ChatMessage, Locale, SIDEBAR_DEFAULT_WIDTH, SIDEBAR_MAX_WIDTH, SIDEBAR_MIN_WIDTH,
        civil_from_days, clamp_sidebar_width, format_turn_timestamp, is_safe_external_url,
        parse_tz_offset_hhmm, sidebar_text_units,
    };

    #[test]
    fn 사이드바_폭_클램프와_동적_말줄임_계산() {
        assert_eq!(clamp_sidebar_width(120.0, 1240.0), SIDEBAR_MIN_WIDTH);
        assert_eq!(clamp_sidebar_width(600.0, 1240.0), SIDEBAR_MAX_WIDTH);
        assert_eq!(clamp_sidebar_width(340.4, 1240.0), 340.0);
        // 좁은 창(660px)에서는 본문 최소폭(360px)을 보장하기 위해 300px까지만 허용된다.
        assert_eq!(clamp_sidebar_width(420.0, 660.0), 300.0);

        assert_eq!(sidebar_text_units(SIDEBAR_DEFAULT_WIDTH, 28), 28);
        assert!(sidebar_text_units(420.0, 28) > 40);
        assert!(sidebar_text_units(SIDEBAR_MIN_WIDTH, 28) < 28);
    }

    #[test]
    fn 경량_sse_빈_full_text_증분_누적() {
        let mut msg = ChatMessage::model(String::new(), String::new(), String::new());
        assert!(!msg.push_delta("안녕", "", true));
        assert!(!msg.push_delta("하세요", "", true));
        assert_eq!(msg.raw_text, "안녕하세요");
        // 전체 교체 패킷이 오면 덮어쓴다
        assert!(!msg.push_delta("", "최종 문장", false));
        assert_eq!(msg.raw_text, "최종 문장");
    }

    #[test]
    fn allows_only_http_and_https_schemes() {
        assert!(is_safe_external_url("https://gemini.google.com/app"));
        assert!(is_safe_external_url("http://localhost:8080/test"));
        assert!(!is_safe_external_url("file:///etc/passwd"));
        assert!(!is_safe_external_url("javascript:alert(1)"));
        assert!(!is_safe_external_url("ssh://user@host"));
        assert!(!is_safe_external_url("https://example.com\nmalicious"));
        assert!(!is_safe_external_url(""));
    }

    #[test]
    fn 타임존_오프셋과_그레고리력_타임스탬프_포맷() {
        assert_eq!(parse_tz_offset_hhmm("+0900"), Some(32_400));
        assert_eq!(parse_tz_offset_hhmm("-0530"), Some(-19_800));
        assert_eq!(parse_tz_offset_hhmm("invalid"), None);
        // 5바이트 비ASCII 입력에서 UTF-8 슬라이싱 패닉이 발생하지 않아야 한다.
        assert_eq!(parse_tz_offset_hhmm("+가0"), None);
        assert_eq!(civil_from_days(0), (1970, 1, 1));
        assert_eq!(civil_from_days(20_555), (2026, 4, 12));

        // 2024-03-15 05:30:00 UTC == KST(+0900) 2024-03-15 14:30 (오후 2:30)
        let ts = 1_710_480_600i64;
        let ko = format_turn_timestamp(ts, 32_400, Locale::Ko);
        let en = format_turn_timestamp(ts, 32_400, Locale::En);
        assert!(ko.contains("2024년 3월 15일") && ko.contains("오후 2:30"), "ko={ko}");
        assert!(en.contains("Mar 15, 2024") && en.contains("2:30 PM"), "en={en}");
    }
}
