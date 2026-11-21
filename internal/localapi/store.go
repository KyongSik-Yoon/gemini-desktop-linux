package localapi

import (
	"context"
	"crypto/rand"
	"encoding/hex"
	"encoding/json"
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"sort"
	"strings"
	"sync"
	"time"

	"github.com/sam/gemini-remote-linux/internal/config"
	"github.com/sam/gemini-remote-linux/internal/proto"
	"github.com/sam/gemini-remote-linux/internal/robin"
)

// Notebook은 사용자가 소스와 메모를 묶어 Gemini와 대화할 수 있는 노트북이다.
type Notebook struct {
	ID              string   `json:"id"`
	Title           string   `json:"title"`
	Description     string   `json:"description,omitempty"`
	Sources         []string `json:"sources,omitempty"`
	Notes           string   `json:"notes,omitempty"`
	SourceCount     int      `json:"source_count,omitempty"`
	IsRemote        bool     `json:"is_remote,omitempty"`
	ConversationIDs []string `json:"conversation_ids,omitempty"`
	UpdatedAtUnix   int64    `json:"updated_at_unix"`
}

// ScheduledTask는 지정 시각·주기에 자동으로 실행되는 예약 프롬프트 작업이다.
type ScheduledTask struct {
	ID                 string `json:"id"`
	Title              string `json:"title"`
	Prompt             string `json:"prompt"`
	ScheduleType       string `json:"schedule_type"` // daily, weekly, hourly, once
	TimeOfDay          string `json:"time_of_day"`   // HH:MM
	ScheduleDesc       string `json:"schedule_desc,omitempty"`
	IsRemote           bool   `json:"is_remote,omitempty"`
	Enabled            bool   `json:"enabled"`
	LastRunUnix        int64  `json:"last_run_unix,omitempty"`
	LastConversationID string `json:"last_conversation_id,omitempty"`
	LastStatus         string `json:"last_status,omitempty"`
	LastExcerpt        string `json:"last_excerpt,omitempty"`
	CreatedAtUnix      int64  `json:"created_at_unix"`
}

// LibraryEntry는 라이브러리 탭에 저장된 문서(Canvas)·이미지·첨부 파일 항목이다.
type LibraryEntry struct {
	ID             string `json:"id"`
	Kind           string `json:"kind"` // document, image, file, prompt, audio
	Title          string `json:"title"`
	Path           string `json:"path,omitempty"`
	URL            string `json:"url,omitempty"`
	Preview        string `json:"preview,omitempty"`
	ContentPreview string `json:"content_preview,omitempty"`
	IsRemote       bool   `json:"is_remote,omitempty"`
	ConversationID string `json:"conversation_id,omitempty"`
	CreatedAtUnix  int64  `json:"created_at_unix"`
}

// UIPreferences는 앱의 영속 환경설정(모델, 사고 수준, 테마, 언어, 개인 인텔리전스, 지침, 트레이 최소화 등)이다.
type UIPreferences struct {
	SelectedModelID      string              `json:"selected_model_id,omitempty"`
	ThinkingLevel        int32               `json:"thinking_level,omitempty"` // 0: 자동, 1: 낮음, 4: 보통, 2: 높음
	ThemeMode            string              `json:"theme_mode,omitempty"`     // system, light, dark
	Language             string              `json:"language,omitempty"`       // system, ko, en
	PersonalIntelligence bool                `json:"personal_intelligence"`
	CustomInstructions   string              `json:"custom_instructions,omitempty"`
	CloseToTray          *bool               `json:"close_to_tray,omitempty"`
	UnreadOverrides      map[string]bool     `json:"unread_overrides,omitempty"`
	NotebookLinks        map[string][]string `json:"notebook_links,omitempty"`
}

// WorkspaceStore는 노트북·예약 작업·라이브러리·UI 설정을 ~/.config/gemini-remote/ 하위에 영속 저장한다.
type WorkspaceStore struct {
	mu            sync.Mutex
	dir           string
	notebooks     []Notebook
	schedules     []ScheduledTask
	library       []LibraryEntry
	prefs         UIPreferences
	runningScheds map[string]bool
}

func newWorkspaceStore() *WorkspaceStore {
	dir, err := config.ConfigDir()
	if err != nil || dir == "" {
		home, _ := os.UserHomeDir()
		dir = filepath.Join(home, ".config", "gemini-remote")
	}
	_ = os.MkdirAll(dir, 0o700)
	st := &WorkspaceStore{
		dir:           dir,
		runningScheds: make(map[string]bool),
	}
	st.load()
	return st
}

func genID(prefix string) string {
	var b [6]byte
	_, _ = rand.Read(b[:])
	return prefix + "_" + hex.EncodeToString(b[:])
}

func (st *WorkspaceStore) load() {
	readJSON := func(name string, dst any) {
		p := filepath.Join(st.dir, name)
		data, err := os.ReadFile(p)
		if err == nil {
			_ = json.Unmarshal(data, dst)
		}
	}
	readJSON("notebooks.json", &st.notebooks)
	readJSON("schedules.json", &st.schedules)
	readJSON("library.json", &st.library)
	readJSON("ui_prefs.json", &st.prefs)
	if st.prefs.ThemeMode == "" {
		st.prefs.ThemeMode = "system"
	}
	if st.prefs.Language == "" {
		st.prefs.Language = "system"
	}
	if st.prefs.ThinkingLevel == 0 {
		st.prefs.ThinkingLevel = proto.ThinkingLevelMedium
	}
	if st.prefs.CloseToTray == nil {
		defaultTray := true
		st.prefs.CloseToTray = &defaultTray
	}
	if st.prefs.UnreadOverrides == nil {
		st.prefs.UnreadOverrides = map[string]bool{}
	}
	if st.prefs.NotebookLinks == nil {
		st.prefs.NotebookLinks = map[string][]string{}
	}
}

// --- UI 환경설정 ---

func (st *WorkspaceStore) GetPreferences() UIPreferences {
	st.mu.Lock()
	defer st.mu.Unlock()
	cp := st.prefs
	if st.prefs.CloseToTray != nil {
		v := *st.prefs.CloseToTray
		cp.CloseToTray = &v
	}
	cp.UnreadOverrides = make(map[string]bool, len(st.prefs.UnreadOverrides))
	for k, v := range st.prefs.UnreadOverrides {
		cp.UnreadOverrides[k] = v
	}
	cp.NotebookLinks = make(map[string][]string, len(st.prefs.NotebookLinks))
	for k, v := range st.prefs.NotebookLinks {
		list := make([]string, len(v))
		copy(list, v)
		cp.NotebookLinks[k] = list
	}
	return cp
}

func (st *WorkspaceStore) PatchPreferences(patch map[string]any) (UIPreferences, error) {
	st.mu.Lock()
	defer st.mu.Unlock()
	if v, ok := patch["selected_model_id"].(string); ok {
		st.prefs.SelectedModelID = strings.TrimSpace(v)
	}
	if v, ok := patch["thinking_level"].(float64); ok {
		st.prefs.ThinkingLevel = int32(v)
	}
	if v, ok := patch["theme_mode"].(string); ok {
		switch v {
		case "system", "light", "dark":
			st.prefs.ThemeMode = v
		}
	}
	if v, ok := patch["language"].(string); ok {
		switch v {
		case "system", "ko", "en":
			st.prefs.Language = v
		}
	}
	if v, ok := patch["personal_intelligence"].(bool); ok {
		st.prefs.PersonalIntelligence = v
	}
	if v, ok := patch["custom_instructions"].(string); ok {
		st.prefs.CustomInstructions = v
	}
	if v, ok := patch["close_to_tray"].(bool); ok {
		b := v
		st.prefs.CloseToTray = &b
	}
	if err := st.saveFileLocked("ui_prefs.json", st.prefs); err != nil {
		return st.prefs, err
	}
	return st.prefs, nil
}

func (st *WorkspaceStore) SetUnreadOverride(convID string, unread bool) error {
	st.mu.Lock()
	defer st.mu.Unlock()
	if st.prefs.UnreadOverrides == nil {
		st.prefs.UnreadOverrides = map[string]bool{}
	}
	st.prefs.UnreadOverrides[convID] = unread
	return st.saveFileLocked("ui_prefs.json", st.prefs)
}

func (st *WorkspaceStore) AddConversationToNotebook(notebookID, convID string) error {
	st.mu.Lock()
	defer st.mu.Unlock()
	if st.prefs.NotebookLinks == nil {
		st.prefs.NotebookLinks = map[string][]string{}
	}
	existing := st.prefs.NotebookLinks[notebookID]
	for _, id := range existing {
		if id == convID {
			return nil
		}
	}
	st.prefs.NotebookLinks[notebookID] = append(existing, convID)
	for i := range st.notebooks {
		if st.notebooks[i].ID == notebookID {
			st.notebooks[i].ConversationIDs = append(st.notebooks[i].ConversationIDs, convID)
			st.notebooks[i].UpdatedAtUnix = time.Now().Unix()
			_ = st.saveFileLocked("notebooks.json", st.notebooks)
			break
		}
	}
	return st.saveFileLocked("ui_prefs.json", st.prefs)
}

func (st *WorkspaceStore) saveFileLocked(name string, src any) error {
	data, err := json.MarshalIndent(src, "", "  ")
	if err != nil {
		return err
	}
	p := filepath.Join(st.dir, name)
	f, err := os.CreateTemp(st.dir, name+".*.tmp")
	if err != nil {
		return err
	}
	tmpPath := f.Name()
	if err := f.Chmod(0o600); err != nil {
		_ = f.Close()
		_ = os.Remove(tmpPath)
		return err
	}
	if _, err := f.Write(data); err != nil {
		_ = f.Close()
		_ = os.Remove(tmpPath)
		return err
	}
	if err := f.Close(); err != nil {
		_ = os.Remove(tmpPath)
		return err
	}
	if err := os.Rename(tmpPath, p); err != nil {
		_ = os.Remove(tmpPath)
		return err
	}
	return nil
}

// --- 노트북 ---

func (st *WorkspaceStore) ListNotebooks() []Notebook {
	st.mu.Lock()
	defer st.mu.Unlock()
	out := make([]Notebook, len(st.notebooks))
	copy(out, st.notebooks)
	sort.Slice(out, func(i, j int) bool {
		return out[i].UpdatedAtUnix > out[j].UpdatedAtUnix
	})
	return out
}

func (st *WorkspaceStore) UpsertNotebook(nb Notebook) (Notebook, error) {
	st.mu.Lock()
	defer st.mu.Unlock()
	if strings.TrimSpace(nb.Title) == "" {
		nb.Title = "제목 없는 노트북"
	}
	nb.UpdatedAtUnix = time.Now().Unix()
	if nb.ID == "" {
		nb.ID = genID("nb")
		st.notebooks = append(st.notebooks, nb)
	} else {
		updated := false
		for i := range st.notebooks {
			if st.notebooks[i].ID == nb.ID {
				st.notebooks[i] = nb
				updated = true
				break
			}
		}
		if !updated {
			st.notebooks = append(st.notebooks, nb)
		}
	}
	return nb, st.saveFileLocked("notebooks.json", st.notebooks)
}

func (st *WorkspaceStore) DeleteNotebook(id string) error {
	st.mu.Lock()
	defer st.mu.Unlock()
	filtered := st.notebooks[:0]
	for _, nb := range st.notebooks {
		if nb.ID != id {
			filtered = append(filtered, nb)
		}
	}
	st.notebooks = filtered
	if _, hadLinks := st.prefs.NotebookLinks[id]; hadLinks {
		delete(st.prefs.NotebookLinks, id)
		_ = st.saveFileLocked("ui_prefs.json", st.prefs)
	}
	return st.saveFileLocked("notebooks.json", st.notebooks)
}

// --- 예약 작업 ---

func (st *WorkspaceStore) ListSchedules() []ScheduledTask {
	st.mu.Lock()
	defer st.mu.Unlock()
	out := make([]ScheduledTask, len(st.schedules))
	copy(out, st.schedules)
	sort.Slice(out, func(i, j int) bool {
		return out[i].CreatedAtUnix > out[j].CreatedAtUnix
	})
	return out
}

func (st *WorkspaceStore) CreateSchedule(task ScheduledTask) (ScheduledTask, error) {
	st.mu.Lock()
	defer st.mu.Unlock()
	if strings.TrimSpace(task.Title) == "" {
		task.Title = "예약 작업"
	}
	if task.ScheduleType == "" {
		task.ScheduleType = "daily"
	}
	if task.TimeOfDay == "" {
		task.TimeOfDay = "09:00"
	}
	task.ID = genID("sch")
	task.Enabled = true
	task.CreatedAtUnix = time.Now().Unix()
	st.schedules = append(st.schedules, task)
	return task, st.saveFileLocked("schedules.json", st.schedules)
}

func (st *WorkspaceStore) SetScheduleEnabled(id string, enabled bool) (*ScheduledTask, error) {
	st.mu.Lock()
	defer st.mu.Unlock()
	for i := range st.schedules {
		if st.schedules[i].ID == id {
			st.schedules[i].Enabled = enabled
			t := st.schedules[i]
			return &t, st.saveFileLocked("schedules.json", st.schedules)
		}
	}
	return nil, fmt.Errorf("예약 작업을 찾을 수 없습니다: %s", id)
}

func (st *WorkspaceStore) DeleteSchedule(id string) error {
	st.mu.Lock()
	defer st.mu.Unlock()
	filtered := st.schedules[:0]
	for _, s := range st.schedules {
		if s.ID != id {
			filtered = append(filtered, s)
		}
	}
	st.schedules = filtered
	return st.saveFileLocked("schedules.json", st.schedules)
}

func (st *WorkspaceStore) GetSchedule(id string) (*ScheduledTask, bool) {
	st.mu.Lock()
	defer st.mu.Unlock()
	for i := range st.schedules {
		if st.schedules[i].ID == id {
			t := st.schedules[i]
			return &t, true
		}
	}
	return nil, false
}

func (st *WorkspaceStore) RecordScheduleRun(id, convID, status, excerpt string, disableAfter bool) {
	st.mu.Lock()
	defer st.mu.Unlock()
	for i := range st.schedules {
		if st.schedules[i].ID == id {
			st.schedules[i].LastRunUnix = time.Now().Unix()
			if convID != "" {
				st.schedules[i].LastConversationID = convID
			}
			st.schedules[i].LastStatus = status
			if len([]rune(excerpt)) > 160 {
				excerpt = string([]rune(excerpt)[:160]) + "…"
			}
			st.schedules[i].LastExcerpt = excerpt
			if disableAfter {
				st.schedules[i].Enabled = false
			}
			_ = st.saveFileLocked("schedules.json", st.schedules)
			return
		}
	}
}

// --- 라이브러리 ---

func (st *WorkspaceStore) ListLibrary() []LibraryEntry {
	st.mu.Lock()
	defer st.mu.Unlock()
	out := make([]LibraryEntry, len(st.library))
	copy(out, st.library)
	sort.Slice(out, func(i, j int) bool {
		return out[i].CreatedAtUnix > out[j].CreatedAtUnix
	})
	return out
}

func (st *WorkspaceStore) AddLibraryEntry(entry LibraryEntry) {
	st.AddLibraryEntries([]LibraryEntry{entry})
}

// AddLibraryEntries는 여러 라이브러리 항목을 단일 락·단일 파일 쓰기로 배치 저장한다.
func (st *WorkspaceStore) AddLibraryEntries(entries []LibraryEntry) {
	if len(entries) == 0 {
		return
	}
	st.mu.Lock()
	defer st.mu.Unlock()
	now := time.Now().Unix()
	for _, entry := range entries {
		if entry.ID == "" {
			entry.ID = genID("lib")
		}
		if entry.CreatedAtUnix == 0 {
			entry.CreatedAtUnix = now
		}
		st.library = append(st.library, entry)
	}
	if len(st.library) > 200 {
		st.library = st.library[len(st.library)-200:]
	}
	_ = st.saveFileLocked("library.json", st.library)
}

func (st *WorkspaceStore) finishScheduleRun(id string) {
	st.mu.Lock()
	delete(st.runningScheds, id)
	st.mu.Unlock()
}

// RunScheduler는 데몬이 살아있는 동안 30초마다 예약 시각이 도래한 작업을 자동 실행한다.
func (st *WorkspaceStore) RunScheduler(ctx context.Context, robinCli *robin.Client) {
	if robinCli == nil {
		return
	}
	ticker := time.NewTicker(30 * time.Second)
	defer ticker.Stop()

	for {
		select {
		case <-ctx.Done():
			return
		case now := <-ticker.C:
			due := st.collectDueSchedules(now)
			for _, task := range due {
				t := task
				go func() {
					defer st.finishScheduleRun(t.ID)
					runCtx, cancel := context.WithTimeout(ctx, 2*time.Minute)
					defer cancel()
					final, err := robinCli.StreamQuery(runCtx, proto.ChatQueryParams{
						Text: t.Prompt,
					}, nil)
					if err != nil {
						st.RecordScheduleRun(t.ID, "", "오류: "+err.Error(), "", false)
						notifyDesktopLocal("Gemini 예약 오류", t.Title)
						return
					}
					st.RecordScheduleRun(t.ID, final.ConversationID, "완료", final.Text, t.ScheduleType == "once")
					notifyDesktopLocal("Gemini 예약 완료", t.Title)
				}()
			}
		}
	}
}

func notifyDesktopLocal(title, body string) {
	if os.Getenv("GEMINI_DISABLE_NOTIFICATIONS") == "1" {
		return
	}
	bin, err := exec.LookPath("notify-send")
	if err != nil {
		return
	}
	go func() {
		ctx, cancel := context.WithTimeout(context.Background(), 2*time.Second)
		defer cancel()
		_ = exec.CommandContext(ctx, bin, "--app-name=Gemini", title, body).Run()
	}()
}

func (st *WorkspaceStore) collectDueSchedules(now time.Time) []ScheduledTask {
	st.mu.Lock()
	defer st.mu.Unlock()
	if st.runningScheds == nil {
		st.runningScheds = make(map[string]bool)
	}
	hhmm := now.Format("15:04")
	var due []ScheduledTask
	for _, s := range st.schedules {
		if !s.Enabled || strings.TrimSpace(s.Prompt) == "" || st.runningScheds[s.ID] {
			continue
		}
		last := time.Unix(s.LastRunUnix, 0)
		matched := false
		switch s.ScheduleType {
		case "hourly":
			matched = now.Sub(last) >= 59*time.Minute
		case "weekly":
			matched = s.TimeOfDay == hhmm && now.Sub(last) >= 6*24*time.Hour
		case "once":
			matched = s.LastRunUnix == 0 && s.TimeOfDay == hhmm
		default: // daily
			matched = s.TimeOfDay == hhmm && (s.LastRunUnix == 0 || last.Format("2006-01-02") != now.Format("2006-01-02"))
		}
		if matched {
			st.runningScheds[s.ID] = true
			due = append(due, s)
		}
	}
	return due
}
