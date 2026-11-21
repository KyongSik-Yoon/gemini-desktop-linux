package localapi

import (
	"bytes"
	"context"
	"encoding/json"
	"errors"
	"net"
	"net/http"
	"net/http/httptest"
	"path/filepath"
	"sync/atomic"
	"testing"
	"time"

	"github.com/sam/gemini-remote-linux/internal/config"
	"github.com/sam/gemini-remote-linux/internal/proto"
	"github.com/sam/gemini-remote-linux/internal/robin"
)

func TestLocalAPIServerHealth(t *testing.T) {
	sock := filepath.Join(t.TempDir(), "test.sock")
	cfg := &config.Config{
		DeviceID:   "TEST-DEV-ID",
		DeviceName: "TestLinux",
	}
	srv := NewServer(cfg, nil, sock)

	ctx, cancel := context.WithCancel(context.Background())
	defer cancel()
	errCh := make(chan error, 1)
	go func() {
		errCh <- srv.Serve(ctx)
	}()

	cli := &http.Client{
		Transport: &http.Transport{
			DialContext: func(ctx context.Context, _, _ string) (net.Conn, error) {
				return net.Dial("unix", sock)
			},
		},
		Timeout: 2 * time.Second,
	}

	var resp *http.Response
	var err error
	for i := 0; i < 20; i++ {
		resp, err = cli.Get("http://unix/health")
		if err == nil {
			break
		}
		time.Sleep(25 * time.Millisecond)
	}
	if err != nil {
		t.Fatalf("/health 호출 실패: %v", err)
	}
	defer resp.Body.Close()

	var body map[string]any
	if err := json.NewDecoder(resp.Body).Decode(&body); err != nil {
		t.Fatalf("JSON 디코딩 실패: %v", err)
	}
	if body["ok"] != true || body["device_id"] != "TEST-DEV-ID" {
		t.Fatalf("예상치 못한 응답: %+v", body)
	}

	cancel()
	if err := <-errCh; err != nil {
		t.Fatalf("서버 종료 오류: %v", err)
	}
}

func TestSingleflightCancelIsolation(t *testing.T) {
	var sf singleflightGroup[string]
	started := make(chan struct{})
	release := make(chan struct{})
	var calls atomic.Int32

	ctx1, cancel1 := context.WithCancel(context.Background())
	errCh1 := make(chan error, 1)
	go func() {
		_, err := sf.Do(ctx1, "k", func(runCtx context.Context) (string, error) {
			calls.Add(1)
			close(started)
			<-release
			if runCtx.Err() != nil {
				return "", runCtx.Err()
			}
			return "ok-value", nil
		})
		errCh1 <- err
	}()

	<-started

	resCh2 := make(chan string, 1)
	errCh2 := make(chan error, 1)
	go func() {
		v, err := sf.Do(context.Background(), "k", func(runCtx context.Context) (string, error) {
			calls.Add(1)
			return "should-not-run", nil
		})
		resCh2 <- v
		errCh2 <- err
	}()

	// 첫 호출자를 취소해도 공용 실행과 두 번째 대기자는 영향을 받지 않아야 한다.
	time.Sleep(15 * time.Millisecond)
	cancel1()
	if err := <-errCh1; !errors.Is(err, context.Canceled) {
		t.Fatalf("첫 호출자는 context.Canceled를 받아야 함: %v", err)
	}

	close(release)
	if err := <-errCh2; err != nil {
		t.Fatalf("두 번째 대기자는 정상 완료되어야 함: %v", err)
	}
	if got := <-resCh2; got != "ok-value" {
		t.Fatalf("예상치 못한 결과: %q", got)
	}
	if calls.Load() != 1 {
		t.Fatalf("singleflight는 1회만 실행되어야 함: %d", calls.Load())
	}
}

func TestWorkspaceStoreNotebookCleanupBatchAndSchedulerDedup(t *testing.T) {
	st := &WorkspaceStore{
		dir:           t.TempDir(),
		runningScheds: make(map[string]bool),
	}
	st.load()

	nb, err := st.UpsertNotebook(Notebook{Title: "테스트 노트북"})
	if err != nil {
		t.Fatalf("노트북 생성 실패: %v", err)
	}
	if err := st.AddConversationToNotebook(nb.ID, "c_1"); err != nil {
		t.Fatalf("대화 연결 실패: %v", err)
	}
	if err := st.DeleteNotebook(nb.ID); err != nil {
		t.Fatalf("노트북 삭제 실패: %v", err)
	}
	prefs := st.GetPreferences()
	if _, exists := prefs.NotebookLinks[nb.ID]; exists {
		t.Fatalf("삭제된 노트북의 고아 링크가 남아 있음")
	}

	st.AddLibraryEntries([]LibraryEntry{
		{Kind: "image", Title: "a.png"},
		{Kind: "file", Title: "b.txt"},
	})
	if len(st.ListLibrary()) != 2 {
		t.Fatalf("배치 저장 개수가 2여야 함: %d", len(st.ListLibrary()))
	}

	now := time.Date(2026, 11, 21, 9, 0, 10, 0, time.Local)
	task, err := st.CreateSchedule(ScheduledTask{
		Title:        "아침 브리핑",
		Prompt:       "요약해줘",
		ScheduleType: "daily",
		TimeOfDay:    "09:00",
	})
	if err != nil {
		t.Fatalf("예약 작업 생성 실패: %v", err)
	}
	due1 := st.collectDueSchedules(now)
	if len(due1) != 1 || due1[0].ID != task.ID {
		t.Fatalf("첫 수집에서 1건이 나와야 함: %+v", due1)
	}
	// 같은 1분 내 30초 뒤 틱은 in-flight 마킹으로 중복 실행되지 않아야 한다.
	due2 := st.collectDueSchedules(now.Add(30 * time.Second))
	if len(due2) != 0 {
		t.Fatalf("실행 중인 작업이 중복 수집됨: %+v", due2)
	}
	st.finishScheduleRun(task.ID)
}

func TestEnrichGeneratedImagesRespectsPlaceholderCount(t *testing.T) {
	srv := &Server{
		robinCli:     &robin.Client{},
		mediaCacheAt: time.Now(),
		mediaCache: []proto.AppArtifact{
			{ConversationID: "c_1", URL: "https://example.com/img1.png", Title: "img1", CreatedAtUnix: 1005},
			{ConversationID: "c_1", URL: "https://example.com/img2.png", Title: "img2", CreatedAtUnix: 1020},
		},
	}
	turns := []proto.ConversationTurn{
		{
			TimestampUnix: 1000,
			ModelText:     "첫 번째 이미지: http://googleusercontent.com/image_generation_content/0",
		},
		{
			TimestampUnix: 1015,
			ModelText:     "두 번째 이미지: http://googleusercontent.com/image_generation_content/0",
		},
	}

	srv.enrichGeneratedImages(context.Background(), "c_1", turns)

	if len(turns[0].GeneratedImages) != 1 || turns[0].GeneratedImages[0].URL != "https://example.com/img1.png" {
		t.Fatalf("첫 턴이 플레이스홀더 1개만 매칭해야 함: %+v", turns[0].GeneratedImages)
	}
	if len(turns[1].GeneratedImages) != 1 || turns[1].GeneratedImages[0].URL != "https://example.com/img2.png" {
		t.Fatalf("둘째 턴이 남은 이미지 1개를 매칭해야 함: %+v", turns[1].GeneratedImages)
	}
}

func TestHandleFeedbackRequiresConversationID(t *testing.T) {
	srv := NewServer(&config.Config{}, nil, filepath.Join(t.TempDir(), "s.sock"))
	req := httptest.NewRequest(http.MethodPost, "/v1/feedback", bytes.NewBufferString(`{"positive":true}`))
	rec := httptest.NewRecorder()
	srv.handleFeedback(rec, req)
	if rec.Code != http.StatusBadRequest {
		t.Fatalf("conversation_id 누락 시 400이어야 함: got %d", rec.Code)
	}
}

func TestPreferencesCloseToTrayDefaultAndPatch(t *testing.T) {
	dir := t.TempDir()
	st := &WorkspaceStore{
		dir:           dir,
		runningScheds: make(map[string]bool),
	}
	st.load()

	prefs := st.GetPreferences()
	if prefs.CloseToTray == nil || !*prefs.CloseToTray {
		t.Fatalf("기본 close_to_tray는 true여야 함: %+v", prefs.CloseToTray)
	}

	updated, err := st.PatchPreferences(map[string]any{"close_to_tray": false})
	if err != nil {
		t.Fatalf("PatchPreferences 실패: %v", err)
	}
	if updated.CloseToTray == nil || *updated.CloseToTray {
		t.Fatalf("close_to_tray=false 패치 실패: %+v", updated.CloseToTray)
	}

	// 디스크에서 재로드해도 false가 유지되어야 한다.
	st2 := &WorkspaceStore{
		dir:           dir,
		runningScheds: make(map[string]bool),
	}
	st2.load()
	reloaded := st2.GetPreferences()
	if reloaded.CloseToTray == nil || *reloaded.CloseToTray {
		t.Fatalf("재로드 후 close_to_tray=false가 유지되어야 함: %+v", reloaded.CloseToTray)
	}
}
