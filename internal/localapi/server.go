package localapi

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"net"
	"net/http"
	"os"
	"path/filepath"
	"strconv"
	"strings"
	"sync"
	"time"

	"github.com/sam/gemini-remote-linux/internal/config"
	"github.com/sam/gemini-remote-linux/internal/proto"
	"github.com/sam/gemini-remote-linux/internal/robin"
)

// Server는 로컬 데스크톱 앱(Rust iced 등)과 CLI가 통신할 Unix 도메인 소켓 HTTP/SSE 서버다.
type Server struct {
	cfg        *config.Config
	robinCli   *robin.Client
	sockPath   string
	httpSrv    *http.Server
	store      *WorkspaceStore
	remoteCtrl RemoteController

	avatarMu    sync.Mutex
	avatarBytes []byte
	avatarType  string
	avatarURL   string

	mediaMu      sync.Mutex
	mediaCache   []proto.AppArtifact
	mediaCacheAt time.Time
}

func NewServer(cfg *config.Config, robinCli *robin.Client, sockPath string) *Server {
	s := &Server{
		cfg:      cfg,
		robinCli: robinCli,
		sockPath: sockPath,
		store:    newWorkspaceStore(),
	}
	mux := http.NewServeMux()
	mux.HandleFunc("GET /health", s.handleHealth)
	mux.HandleFunc("GET /v1/profile", s.handleGetProfile)
	mux.HandleFunc("GET /v1/profile/avatar", s.handleGetAvatar)
	mux.HandleFunc("GET /v1/usage", s.handleGetUsage)
	mux.HandleFunc("GET /v1/preferences", s.handleGetPreferences)
	mux.HandleFunc("PATCH /v1/preferences", s.handlePatchPreferences)
	mux.HandleFunc("GET /v1/models", s.handleGetModels)
	mux.HandleFunc("GET /v1/bots", s.handleListBots)
	mux.HandleFunc("GET /v1/devices", s.handleListDevices)
	mux.HandleFunc("GET /v1/remote/status", s.handleGetRemoteStatus)
	mux.HandleFunc("GET /v1/remote/events", s.handleRemoteEvents)
	mux.HandleFunc("POST /v1/remote/toggle", s.handleToggleRemote)
	mux.HandleFunc("POST /v1/remote/register", s.handleRegisterRemote)
	mux.HandleFunc("POST /v1/remote/folders", s.handleAddRemoteFolder)
	mux.HandleFunc("DELETE /v1/remote/folders", s.handleRemoveRemoteFolder)
	mux.HandleFunc("GET /v1/conversations", s.handleListConversations)
	mux.HandleFunc("GET /v1/conversations/search", s.handleSearchConversations)
	mux.HandleFunc("GET /v1/conversations/{id}/turns", s.handleListTurns)
	mux.HandleFunc("POST /v1/conversations/{id}/share", s.handleShareConversation)
	mux.HandleFunc("PATCH /v1/conversations/{id}", s.handleRenameConversation)
	mux.HandleFunc("DELETE /v1/conversations/{id}", s.handleDeleteConversation)
	mux.HandleFunc("POST /v1/feedback", s.handleFeedback)
	mux.HandleFunc("GET /v1/notebooks", s.handleListNotebooks)
	mux.HandleFunc("POST /v1/notebooks", s.handleUpsertNotebook)
	mux.HandleFunc("POST /v1/notebooks/{id}/conversations", s.handleAddNotebookConversation)
	mux.HandleFunc("DELETE /v1/notebooks/{id}", s.handleDeleteNotebook)
	mux.HandleFunc("GET /v1/schedules", s.handleListSchedules)
	mux.HandleFunc("POST /v1/schedules", s.handleCreateSchedule)
	mux.HandleFunc("POST /v1/schedules/recommended", s.handleAddRecommendedSchedule)
	mux.HandleFunc("PATCH /v1/schedules/{id}", s.handlePatchSchedule)
	mux.HandleFunc("DELETE /v1/schedules/{id}", s.handleDeleteSchedule)
	mux.HandleFunc("POST /v1/schedules/{id}/run", s.handleRunScheduleNow)
	mux.HandleFunc("GET /v1/library", s.handleListLibrary)
	mux.HandleFunc("GET /v1/library/image", s.handleProxyLibraryImage)
	mux.HandleFunc("POST /v1/chat/stream", s.handleChatStream)
	mux.HandleFunc("POST /v1/chat/abort", s.handleChatAbort)

	s.httpSrv = &http.Server{
		Handler:           mux,
		ReadHeaderTimeout: 10 * time.Second,
	}
	return s
}

// Serve는 Unix 도메인 소켓을 바인딩하고 ctx가 취소될 때까지 요청을 처리한다.
func (s *Server) Serve(ctx context.Context) error {
	if err := os.MkdirAll(filepath.Dir(s.sockPath), 0o700); err != nil {
		return fmt.Errorf("소켓 디렉터리 생성 실패: %w", err)
	}
	_ = os.Remove(s.sockPath)

	ln, err := net.Listen("unix", s.sockPath)
	if err != nil {
		return fmt.Errorf("로컬 소켓 바인딩 실패 (%s): %w", s.sockPath, err)
	}
	_ = os.Chmod(s.sockPath, 0o600)

	go s.store.RunScheduler(ctx, s.robinCli)

	go func() {
		<-ctx.Done()
		shutCtx, cancel := context.WithTimeout(context.Background(), 3*time.Second)
		defer cancel()
		_ = s.httpSrv.Shutdown(shutCtx)
		_ = os.Remove(s.sockPath)
	}()

	fmt.Printf("[localapi] 로컬 IPC 소켓 서버 시작: %s\n", s.sockPath)
	err = s.httpSrv.Serve(ln)
	if errors.Is(err, http.ErrServerClosed) {
		return nil
	}
	return err
}

func writeJSON(w http.ResponseWriter, status int, v any) {
	w.Header().Set("Content-Type", "application/json; charset=utf-8")
	w.WriteHeader(status)
	_ = json.NewEncoder(w).Encode(v)
}

func writeError(w http.ResponseWriter, status int, err error) {
	writeJSON(w, status, map[string]string{"error": err.Error()})
}

func (s *Server) handleHealth(w http.ResponseWriter, r *http.Request) {
	writeJSON(w, http.StatusOK, map[string]any{
		"ok":          true,
		"device_id":   s.cfg.DeviceID,
		"device_name": s.cfg.DeviceName,
		"socket":      s.sockPath,
	})
}

func (s *Server) handleGetProfile(w http.ResponseWriter, r *http.Request) {
	prof, err := s.robinCli.GetUserProfile(r.Context())
	if err != nil {
		writeError(w, http.StatusBadGateway, err)
		return
	}
	writeJSON(w, http.StatusOK, prof)
}

func (s *Server) handleGetAvatar(w http.ResponseWriter, r *http.Request) {
	s.avatarMu.Lock()
	cached := s.avatarBytes
	ct := s.avatarType
	s.avatarMu.Unlock()

	if len(cached) == 0 {
		prof, err := s.robinCli.GetUserProfile(r.Context())
		if err != nil || prof.Picture == "" {
			w.WriteHeader(http.StatusNotFound)
			return
		}
		data, contentType, err := s.robinCli.FetchAvatarBytes(r.Context(), prof.Picture)
		if err != nil {
			w.WriteHeader(http.StatusBadGateway)
			return
		}
		s.avatarMu.Lock()
		s.avatarBytes = data
		s.avatarType = contentType
		s.avatarURL = prof.Picture
		cached = data
		ct = contentType
		s.avatarMu.Unlock()
	}
	w.Header().Set("Content-Type", ct)
	w.WriteHeader(http.StatusOK)
	_, _ = w.Write(cached)
}

func (s *Server) handleGetModels(w http.ResponseWriter, r *http.Request) {
	res, err := s.robinCli.GetUserStatus(r.Context())
	if err != nil {
		writeError(w, http.StatusBadGateway, err)
		return
	}
	writeJSON(w, http.StatusOK, res)
}

func (s *Server) handleListBots(w http.ResponseWriter, r *http.Request) {
	bots, err := s.robinCli.ListBots(r.Context())
	if err != nil {
		writeError(w, http.StatusBadGateway, err)
		return
	}
	writeJSON(w, http.StatusOK, map[string]any{"bots": bots})
}

func (s *Server) handleListDevices(w http.ResponseWriter, r *http.Request) {
	devices, err := s.robinCli.ListDevices(r.Context())
	if err != nil {
		writeError(w, http.StatusBadGateway, err)
		return
	}
	writeJSON(w, http.StatusOK, map[string]any{"devices": devices})
}

func (s *Server) handleListConversations(w http.ResponseWriter, r *http.Request) {
	pageSize := int32(50)
	if ps := r.URL.Query().Get("page_size"); ps != "" {
		if n, err := strconv.Atoi(ps); err == nil && n > 0 && n <= 100 {
			pageSize = int32(n)
		}
	}
	res, err := s.robinCli.ListAllConversations(r.Context(), pageSize)
	if err != nil {
		writeError(w, http.StatusBadGateway, err)
		return
	}
	prefs := s.store.GetPreferences()
	if len(prefs.UnreadOverrides) > 0 {
		for i := range res.Conversations {
			if ov, ok := prefs.UnreadOverrides[res.Conversations[i].ConversationID]; ok {
				res.Conversations[i].HasUnread = ov
			}
		}
	}
	writeJSON(w, http.StatusOK, res)
}

func (s *Server) handleGetUsage(w http.ResponseWriter, r *http.Request) {
	if s.robinCli == nil {
		writeJSON(w, http.StatusOK, &proto.UsageInfo{Buckets: []proto.UsageBucket{}})
		return
	}
	info, err := s.robinCli.GetUsageInfo(r.Context())
	if err != nil {
		writeError(w, http.StatusBadGateway, err)
		return
	}
	writeJSON(w, http.StatusOK, info)
}

func (s *Server) handleGetPreferences(w http.ResponseWriter, r *http.Request) {
	writeJSON(w, http.StatusOK, s.store.GetPreferences())
}

func (s *Server) handlePatchPreferences(w http.ResponseWriter, r *http.Request) {
	var patch map[string]any
	if err := json.NewDecoder(r.Body).Decode(&patch); err != nil {
		writeError(w, http.StatusBadRequest, err)
		return
	}
	updated, err := s.store.PatchPreferences(patch)
	if err != nil {
		writeError(w, http.StatusInternalServerError, err)
		return
	}
	writeJSON(w, http.StatusOK, updated)
}

func (s *Server) handleSearchConversations(w http.ResponseWriter, r *http.Request) {
	q := strings.TrimSpace(r.URL.Query().Get("q"))
	if q == "" {
		writeJSON(w, http.StatusOK, map[string]any{"hits": []proto.SearchConversationHit{}})
		return
	}
	hits, err := s.robinCli.SearchConversations(r.Context(), q, 25)
	if err != nil {
		writeError(w, http.StatusBadGateway, err)
		return
	}
	writeJSON(w, http.StatusOK, map[string]any{"hits": hits})
}

func (s *Server) handleListNotebooks(w http.ResponseWriter, r *http.Request) {
	prefs := s.store.GetPreferences()
	merged := make([]Notebook, 0)
	seen := map[string]bool{}

	if s.robinCli != nil {
		if remotes, err := s.robinCli.ListGeminiNotebooks(r.Context()); err == nil {
			for _, rn := range remotes {
				nb := Notebook{
					ID:              rn.ID,
					Title:           rn.Title,
					SourceCount:     rn.SourceCount,
					IsRemote:        true,
					ConversationIDs: prefs.NotebookLinks[rn.ID],
					UpdatedAtUnix:   rn.UpdatedAtUnix,
				}
				merged = append(merged, nb)
				seen[rn.ID] = true
			}
		}
	}
	for _, local := range s.store.ListNotebooks() {
		if seen[local.ID] {
			continue
		}
		if local.SourceCount == 0 {
			local.SourceCount = len(local.Sources)
		}
		if len(local.ConversationIDs) == 0 {
			local.ConversationIDs = prefs.NotebookLinks[local.ID]
		}
		merged = append(merged, local)
	}
	writeJSON(w, http.StatusOK, map[string]any{"notebooks": merged})
}

func (s *Server) handleUpsertNotebook(w http.ResponseWriter, r *http.Request) {
	var nb Notebook
	if err := json.NewDecoder(r.Body).Decode(&nb); err != nil {
		writeError(w, http.StatusBadRequest, err)
		return
	}
	if nb.ID == "" && s.robinCli != nil && nb.Title != "" {
		if createdRemote, err := s.robinCli.CreateGbhNotebook(r.Context(), nb.Title); err == nil && createdRemote != nil && createdRemote.ID != "" {
			nb.ID = createdRemote.ID
			nb.IsRemote = true
		}
	}
	nb.SourceCount = len(nb.Sources)
	saved, err := s.store.UpsertNotebook(nb)
	if err != nil {
		writeError(w, http.StatusInternalServerError, err)
		return
	}
	writeJSON(w, http.StatusOK, saved)
}

func (s *Server) handleAddNotebookConversation(w http.ResponseWriter, r *http.Request) {
	id := strings.TrimSpace(r.PathValue("id"))
	var req struct {
		ConversationID string `json:"conversation_id"`
	}
	if err := json.NewDecoder(r.Body).Decode(&req); err != nil || id == "" || strings.TrimSpace(req.ConversationID) == "" {
		writeError(w, http.StatusBadRequest, fmt.Errorf("notebook id와 conversation_id가 필요합니다"))
		return
	}
	if err := s.store.AddConversationToNotebook(id, strings.TrimSpace(req.ConversationID)); err != nil {
		writeError(w, http.StatusInternalServerError, err)
		return
	}
	writeJSON(w, http.StatusOK, map[string]bool{"ok": true})
}

func (s *Server) handleDeleteNotebook(w http.ResponseWriter, r *http.Request) {
	id := strings.TrimSpace(r.PathValue("id"))
	if id == "" {
		writeError(w, http.StatusBadRequest, fmt.Errorf("id가 필요합니다"))
		return
	}
	if err := s.store.DeleteNotebook(id); err != nil {
		writeError(w, http.StatusInternalServerError, err)
		return
	}
	writeJSON(w, http.StatusOK, map[string]bool{"ok": true})
}

func (s *Server) handleListSchedules(w http.ResponseWriter, r *http.Request) {
	var (
		wg          sync.WaitGroup
		remoteList  []proto.RemoteSchedule
		recommended []proto.RecommendedSchedule
	)
	if s.robinCli != nil {
		wg.Add(2)
		go func() {
			defer wg.Done()
			remoteList, _ = s.robinCli.ListRemySchedules(r.Context())
		}()
		go func() {
			defer wg.Done()
			recommended, _ = s.robinCli.ListRecommendedSchedules(r.Context())
		}()
		wg.Wait()
	}

	merged := make([]ScheduledTask, 0, len(remoteList)+8)
	seen := map[string]bool{}
	for _, rs := range remoteList {
		merged = append(merged, ScheduledTask{
			ID:                 rs.ID,
			Title:              rs.Title,
			Prompt:             rs.Prompt,
			ScheduleType:       rs.ScheduleType,
			TimeOfDay:          rs.TimeOfDay,
			ScheduleDesc:       rs.ScheduleDesc,
			IsRemote:           true,
			Enabled:            rs.Enabled,
			LastRunUnix:        rs.LastRunUnix,
			LastConversationID: rs.LastConversationID,
			LastStatus:         "서버 예약",
		})
		seen[rs.ID] = true
	}
	for _, local := range s.store.ListSchedules() {
		if seen[local.ID] {
			continue
		}
		merged = append(merged, local)
	}
	if recommended == nil {
		recommended = []proto.RecommendedSchedule{}
	}
	writeJSON(w, http.StatusOK, map[string]any{
		"schedules":   merged,
		"recommended": recommended,
	})
}

func (s *Server) handleCreateSchedule(w http.ResponseWriter, r *http.Request) {
	var task ScheduledTask
	if err := json.NewDecoder(r.Body).Decode(&task); err != nil {
		writeError(w, http.StatusBadRequest, err)
		return
	}
	if strings.TrimSpace(task.Prompt) == "" {
		writeError(w, http.StatusBadRequest, fmt.Errorf("프롬프트가 비어 있습니다"))
		return
	}
	created, err := s.store.CreateSchedule(task)
	if err != nil {
		writeError(w, http.StatusInternalServerError, err)
		return
	}
	writeJSON(w, http.StatusOK, created)
}

func (s *Server) handleAddRecommendedSchedule(w http.ResponseWriter, r *http.Request) {
	var req struct {
		ID          string `json:"id"`
		Title       string `json:"title"`
		Description string `json:"description"`
		Prompt      string `json:"prompt"`
	}
	if err := json.NewDecoder(r.Body).Decode(&req); err != nil {
		writeError(w, http.StatusBadRequest, err)
		return
	}
	if s.robinCli != nil && strings.TrimSpace(req.ID) != "" {
		if err := s.robinCli.AddRecommendedSchedule(r.Context(), req.ID); err == nil {
			writeJSON(w, http.StatusOK, map[string]any{"ok": true, "remote": true})
			return
		}
	}
	prompt := strings.TrimSpace(req.Prompt)
	if prompt == "" {
		prompt = strings.TrimSpace(req.Description)
	}
	if prompt == "" {
		prompt = strings.TrimSpace(req.Title)
	}
	created, err := s.store.CreateSchedule(ScheduledTask{
		Title:        strings.TrimSpace(req.Title),
		Prompt:       prompt,
		ScheduleType: "daily",
		TimeOfDay:    "09:00",
		ScheduleDesc: "매일 오전 9:00에",
	})
	if err != nil {
		writeError(w, http.StatusInternalServerError, err)
		return
	}
	writeJSON(w, http.StatusOK, map[string]any{"ok": true, "remote": false, "schedule": created})
}

func (s *Server) handlePatchSchedule(w http.ResponseWriter, r *http.Request) {
	id := strings.TrimSpace(r.PathValue("id"))
	var req struct {
		Enabled *bool `json:"enabled"`
	}
	if err := json.NewDecoder(r.Body).Decode(&req); err != nil || req.Enabled == nil {
		writeError(w, http.StatusBadRequest, fmt.Errorf("enabled 값이 필요합니다"))
		return
	}
	updated, err := s.store.SetScheduleEnabled(id, *req.Enabled)
	if err != nil {
		writeError(w, http.StatusNotFound, err)
		return
	}
	writeJSON(w, http.StatusOK, updated)
}

func (s *Server) handleDeleteSchedule(w http.ResponseWriter, r *http.Request) {
	id := strings.TrimSpace(r.PathValue("id"))
	if _, ok := s.store.GetSchedule(id); ok {
		if err := s.store.DeleteSchedule(id); err != nil {
			writeError(w, http.StatusInternalServerError, err)
			return
		}
		writeJSON(w, http.StatusOK, map[string]bool{"ok": true})
		return
	}
	if s.robinCli != nil && id != "" {
		if err := s.robinCli.DeleteRemySchedule(r.Context(), id); err != nil {
			writeError(w, http.StatusBadGateway, err)
			return
		}
	}
	writeJSON(w, http.StatusOK, map[string]bool{"ok": true})
}

func (s *Server) handleRunScheduleNow(w http.ResponseWriter, r *http.Request) {
	id := strings.TrimSpace(r.PathValue("id"))
	task, ok := s.store.GetSchedule(id)
	if !ok {
		if s.robinCli != nil && id != "" {
			if err := s.robinCli.ExecuteRemySchedule(r.Context(), id); err == nil {
				writeJSON(w, http.StatusOK, map[string]any{
					"ok":     true,
					"remote": true,
				})
				return
			}
			// 원격 실행 RPC가 실패하면 프롬프트로 직접 스트림 실행 시도
			if remotes, err := s.robinCli.ListRemySchedules(r.Context()); err == nil {
				for _, rs := range remotes {
					if rs.ID == id && strings.TrimSpace(rs.Prompt) != "" {
						final, serr := s.robinCli.StreamQuery(r.Context(), proto.ChatQueryParams{
							Text: rs.Prompt,
						}, nil)
						if serr != nil {
							writeError(w, http.StatusBadGateway, serr)
							return
						}
						writeJSON(w, http.StatusOK, map[string]any{
							"ok":              true,
							"conversation_id": final.ConversationID,
							"title":           final.Title,
							"excerpt":         final.Text,
						})
						return
					}
				}
			}
		}
		writeError(w, http.StatusNotFound, fmt.Errorf("예약 작업을 찾을 수 없습니다"))
		return
	}
	final, err := s.robinCli.StreamQuery(r.Context(), proto.ChatQueryParams{
		Text: task.Prompt,
	}, nil)
	if err != nil {
		s.store.RecordScheduleRun(id, "", "오류: "+err.Error(), "", false)
		writeError(w, http.StatusBadGateway, err)
		return
	}
	s.store.RecordScheduleRun(id, final.ConversationID, "완료", final.Text, false)
	writeJSON(w, http.StatusOK, map[string]any{
		"ok":              true,
		"conversation_id": final.ConversationID,
		"title":           final.Title,
		"excerpt":         final.Text,
	})
}

func (s *Server) handleListLibrary(w http.ResponseWriter, r *http.Request) {
	items := make([]LibraryEntry, 0)
	if s.robinCli != nil {
		if arts, err := s.robinCli.ListGeminiAppArtifacts(r.Context(), 24); err == nil {
			for _, a := range arts {
				items = append(items, LibraryEntry{
					ID:             a.ID,
					Kind:           a.Kind,
					Title:          a.Title,
					URL:            a.URL,
					ContentPreview: a.ContentPreview,
					Preview:        a.ContentPreview,
					IsRemote:       true,
					ConversationID: a.ConversationID,
					CreatedAtUnix:  a.CreatedAtUnix,
				})
			}
		}
	}
	items = append(items, s.store.ListLibrary()...)
	writeJSON(w, http.StatusOK, map[string]any{"items": items})
}

func (s *Server) handleProxyLibraryImage(w http.ResponseWriter, r *http.Request) {
	rawURL := strings.TrimSpace(r.URL.Query().Get("url"))
	if rawURL == "" || s.robinCli == nil {
		w.WriteHeader(http.StatusBadRequest)
		return
	}
	data, ct, err := s.robinCli.FetchRemoteImageBytes(r.Context(), rawURL)
	if err != nil {
		w.WriteHeader(http.StatusBadGateway)
		return
	}
	w.Header().Set("Content-Type", ct)
	w.Header().Set("Cache-Control", "private, max-age=300")
	w.WriteHeader(http.StatusOK)
	_, _ = w.Write(data)
}

func (s *Server) handleShareConversation(w http.ResponseWriter, r *http.Request) {
	convID := strings.TrimSpace(r.PathValue("id"))
	if convID == "" {
		writeError(w, http.StatusBadRequest, fmt.Errorf("conversation_id가 비어 있습니다"))
		return
	}
	var req struct {
		Title string `json:"title"`
	}
	_ = json.NewDecoder(r.Body).Decode(&req)

	if s.robinCli != nil {
		if shareURL, err := s.robinCli.CreateSharedConversation(r.Context(), convID, req.Title); err == nil && shareURL != "" {
			writeJSON(w, http.StatusOK, map[string]any{"ok": true, "url": shareURL, "remote": true})
			return
		}
	}
	// 원격 공유 RPC가 지원되지 않는 대화(예: 임시/로컬)는 웹 링크 폴백 제공
	cleanID := strings.TrimPrefix(convID, "c_")
	writeJSON(w, http.StatusOK, map[string]any{
		"ok":     true,
		"url":    "https://gemini.google.com/app/" + cleanID,
		"remote": false,
	})
}

func (s *Server) handleFeedback(w http.ResponseWriter, r *http.Request) {
	var req struct {
		ConversationID string `json:"conversation_id"`
		RequestID      string `json:"request_id"`
		ResponseID     string `json:"response_id"`
		Positive       bool   `json:"positive"`
		Comment        string `json:"comment"`
	}
	if err := json.NewDecoder(r.Body).Decode(&req); err != nil {
		writeError(w, http.StatusBadRequest, err)
		return
	}
	remoteOK := false
	if s.robinCli != nil && req.ConversationID != "" {
		if err := s.robinCli.WriteFeedback(r.Context(), req.ConversationID, req.RequestID, req.ResponseID, req.Positive, req.Comment); err == nil {
			remoteOK = true
		}
	}
	writeJSON(w, http.StatusOK, map[string]any{"ok": true, "remote": remoteOK})
}

func (s *Server) handleListTurns(w http.ResponseWriter, r *http.Request) {
	convID := strings.TrimSpace(r.PathValue("id"))
	if convID == "" {
		writeError(w, http.StatusBadRequest, fmt.Errorf("conversation_id가 비어 있습니다"))
		return
	}
	pageSize := int32(50)
	if ps := r.URL.Query().Get("page_size"); ps != "" {
		if n, err := strconv.Atoi(ps); err == nil && n > 0 && n <= 100 {
			pageSize = int32(n)
		}
	}
	pageToken := r.URL.Query().Get("page_token")
	res, err := s.robinCli.ListConversationTurns(r.Context(), convID, pageSize, pageToken)
	if err != nil {
		writeError(w, http.StatusBadGateway, err)
		return
	}
	s.enrichGeneratedImages(r.Context(), convID, res.Turns)
	writeJSON(w, http.StatusOK, res)
}

// invalidateMediaCache는 새 이미지·미디어가 생성되었을 때 30초 미디어 캐시를 즉시 비운다.
func (s *Server) invalidateMediaCache() {
	s.mediaMu.Lock()
	s.mediaCacheAt = time.Time{}
	s.mediaMu.Unlock()
}

// getCachedMediaArtifacts는 생성 이미지 아티팩트 목록을 짧게(30초) 캐시해
// 일반 대화 조회 시 불필요한 추가 RPC 지연을 방지한다.
func (s *Server) getCachedMediaArtifacts(ctx context.Context, forceRefresh bool) []proto.AppArtifact {
	const ttl = 30 * time.Second
	s.mediaMu.Lock()
	if !forceRefresh && !s.mediaCacheAt.IsZero() && time.Since(s.mediaCacheAt) < ttl {
		cached := s.mediaCache
		s.mediaMu.Unlock()
		return cached
	}
	s.mediaMu.Unlock()

	media, err := s.robinCli.ListGeminiMediaArtifacts(ctx, 80)
	if err != nil {
		return nil
	}
	s.mediaMu.Lock()
	s.mediaCache = media
	s.mediaCacheAt = time.Now()
	s.mediaMu.Unlock()
	return media
}

// enrichGeneratedImages는 서버 미디어 아티팩트의 실제 생성 이미지 URL을 해당 대화 턴에 매칭해 채운다.
// 플레이스홀더(`image_generation_content/`)가 없는 생성 이미지 턴도 ResponseID/RequestID 정확 매칭으로 복구한다.
func (s *Server) enrichGeneratedImages(ctx context.Context, convID string, turns []proto.ConversationTurn) {
	if s.robinCli == nil || len(turns) == 0 {
		return
	}
	hasPlaceholder := false
	for i := range turns {
		if strings.Contains(turns[i].ModelText, "googleusercontent.com/image_generation_content/") {
			hasPlaceholder = true
			break
		}
	}

	media := s.getCachedMediaArtifacts(ctx, false)
	filterConv := func(all []proto.AppArtifact) []proto.AppArtifact {
		var out []proto.AppArtifact
		for _, m := range all {
			if m.ConversationID == convID && m.URL != "" {
				out = append(out, m)
			}
		}
		return out
	}

	convMedia := filterConv(media)
	// 플레이스홀더가 있는데 캐시에 해당 대화의 미디어가 없으면 1회 강제 갱신(방금 생성된 이미지 대응)
	if len(convMedia) == 0 && hasPlaceholder {
		media = s.getCachedMediaArtifacts(ctx, true)
		convMedia = filterConv(media)
	}
	if len(convMedia) == 0 {
		return
	}

	// 오래된 생성 순서대로 정렬해 다중 이미지 턴에서 플레이스홀더 순번과 맞춘다.
	for i := 0; i < len(convMedia)-1; i++ {
		for j := i + 1; j < len(convMedia); j++ {
			if convMedia[j].CreatedAtUnix < convMedia[i].CreatedAtUnix {
				convMedia[i], convMedia[j] = convMedia[j], convMedia[i]
			}
		}
	}

	used := make([]bool, len(convMedia))
	// 1차: ResponseID / RequestID 정확 매칭 (플레이스홀더 유무와 무관하게 매칭)
	for i := range turns {
		t := &turns[i]
		for mi, m := range convMedia {
			if used[mi] {
				continue
			}
			if (m.ResponseID != "" && m.ResponseID == t.ResponseID) ||
				(m.RequestID != "" && m.RequestID == t.RequestID) {
				t.GeneratedImages = append(t.GeneratedImages, proto.GeneratedImage{
					URL:   m.URL,
					Title: m.Title,
				})
				used[mi] = true
			}
		}
	}
	// 2차: ID가 일치하지 않은 남은 플레이스홀더 턴은 생성 시각 근접 매칭(±180초)
	for i := range turns {
		t := &turns[i]
		if len(t.GeneratedImages) > 0 || !strings.Contains(t.ModelText, "googleusercontent.com/image_generation_content/") {
			continue
		}
		for mi, m := range convMedia {
			if used[mi] {
				continue
			}
			diff := m.CreatedAtUnix - t.TimestampUnix
			if diff < 0 {
				diff = -diff
			}
			if diff <= 180 || len(convMedia) == 1 {
				t.GeneratedImages = append(t.GeneratedImages, proto.GeneratedImage{
					URL:   m.URL,
					Title: m.Title,
				})
				used[mi] = true
			}
		}
	}
}

func (s *Server) handleRenameConversation(w http.ResponseWriter, r *http.Request) {
	convID := strings.TrimSpace(r.PathValue("id"))
	var req struct {
		Title  string `json:"title"`
		Pinned *bool  `json:"pinned"`
		Unread *bool  `json:"unread"`
	}
	if err := json.NewDecoder(r.Body).Decode(&req); err != nil {
		writeError(w, http.StatusBadRequest, err)
		return
	}
	if convID == "" || (strings.TrimSpace(req.Title) == "" && req.Pinned == nil && req.Unread == nil) {
		writeError(w, http.StatusBadRequest, fmt.Errorf("conversation_id와 title, pinned 또는 unread가 필요합니다"))
		return
	}
	if strings.TrimSpace(req.Title) != "" {
		if err := s.robinCli.RenameConversation(r.Context(), convID, req.Title); err != nil {
			writeError(w, http.StatusBadGateway, err)
			return
		}
	}
	if req.Pinned != nil {
		if err := s.robinCli.PinConversation(r.Context(), convID, *req.Pinned); err != nil {
			writeError(w, http.StatusBadGateway, err)
			return
		}
	}
	if req.Unread != nil {
		_ = s.store.SetUnreadOverride(convID, *req.Unread)
		if s.robinCli != nil {
			_ = s.robinCli.MarkConversationUnread(r.Context(), convID, *req.Unread)
		}
	}
	writeJSON(w, http.StatusOK, map[string]bool{"ok": true})
}

func (s *Server) handleDeleteConversation(w http.ResponseWriter, r *http.Request) {
	convID := strings.TrimSpace(r.PathValue("id"))
	if convID == "" {
		writeError(w, http.StatusBadRequest, fmt.Errorf("conversation_id가 비어 있습니다"))
		return
	}
	if err := s.robinCli.DeleteConversation(r.Context(), convID); err != nil {
		writeError(w, http.StatusBadGateway, err)
		return
	}
	writeJSON(w, http.StatusOK, map[string]bool{"ok": true})
}

func (s *Server) handleChatAbort(w http.ResponseWriter, r *http.Request) {
	var req struct {
		ConversationID string `json:"conversation_id"`
		RequestID      string `json:"request_id"`
	}
	if err := json.NewDecoder(r.Body).Decode(&req); err != nil {
		writeError(w, http.StatusBadRequest, err)
		return
	}
	if req.ConversationID == "" || req.RequestID == "" {
		writeError(w, http.StatusBadRequest, fmt.Errorf("conversation_id와 request_id가 필요합니다"))
		return
	}
	if err := s.robinCli.AbortGeneration(r.Context(), req.ConversationID, req.RequestID); err != nil {
		writeError(w, http.StatusBadGateway, err)
		return
	}
	writeJSON(w, http.StatusOK, map[string]bool{"ok": true})
}

func (s *Server) handleChatStream(w http.ResponseWriter, r *http.Request) {
	var params proto.ChatQueryParams
	if err := json.NewDecoder(r.Body).Decode(&params); err != nil {
		writeError(w, http.StatusBadRequest, err)
		return
	}
	if strings.TrimSpace(params.Text) == "" && len(params.FilePaths) == 0 && len(params.Attachments) == 0 {
		writeError(w, http.StatusBadRequest, fmt.Errorf("text 또는 첨부 파일이 필요합니다"))
		return
	}

	flusher, ok := w.(http.Flusher)
	if !ok {
		writeError(w, http.StatusInternalServerError, fmt.Errorf("스트리밍 플러시를 지원하지 않습니다"))
		return
	}

	w.Header().Set("Content-Type", "text/event-stream")
	w.Header().Set("Cache-Control", "no-cache")
	w.Header().Set("Connection", "keep-alive")
	w.WriteHeader(http.StatusOK)
	flusher.Flush()

	writeSSE := func(event string, payload any) error {
		b, err := json.Marshal(payload)
		if err != nil {
			return err
		}
		if _, err := fmt.Fprintf(w, "event: %s\ndata: %s\n\n", event, b); err != nil {
			return err
		}
		flusher.Flush()
		return nil
	}

	final, err := s.robinCli.StreamQuery(r.Context(), params, func(chunk proto.ChatStreamChunk) error {
		return writeSSE("chunk", chunk)
	})
	if err != nil {
		_ = writeSSE("error", map[string]string{"error": err.Error()})
		return
	}
	// 새 응답이 완료되면 생성 이미지 캐시를 무효화해 직후 턴 조회에서 최신 이미지를 즉시 반영한다.
	s.invalidateMediaCache()
	if !params.Temporary && len(params.FilePaths) > 0 {
		for _, fp := range params.FilePaths {
			ext := strings.ToLower(filepath.Ext(fp))
			kind := "file"
			switch ext {
			case ".png", ".jpg", ".jpeg", ".webp", ".gif":
				kind = "image"
			case ".wav", ".mp3", ".m4a", ".ogg", ".flac":
				kind = "audio"
			}
			s.store.AddLibraryEntry(LibraryEntry{
				Kind:           kind,
				Title:          filepath.Base(fp),
				Path:           fp,
				Preview:        params.Text,
				ConversationID: final.ConversationID,
			})
		}
	}
	_ = writeSSE("done", final)
}
