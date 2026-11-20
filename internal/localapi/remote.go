package localapi

import (
	"context"
	"encoding/json"
	"fmt"
	"net/http"
	"strings"
	"time"

	"github.com/sam/gemini-remote-linux/internal/robin"
)

// RemoteMCPServerStatus는 설정된 MCP 서버의 요약 상태다.
type RemoteMCPServerStatus struct {
	ServerID    string `json:"server_id"`
	DisplayName string `json:"display_name"`
	Enabled     bool   `json:"enabled"`
}

// RemoteStatusSnapshot은 이 PC 원격 제어 데몬의 실시간 상태 스냅샷이다.
type RemoteStatusSnapshot struct {
	DeviceID          string                  `json:"device_id"`
	DeviceName        string                  `json:"device_name"`
	RemoteEnabled     bool                    `json:"remote_enabled"`
	PunctualConnected bool                    `json:"punctual_connected"`
	ActiveSessions    []string                `json:"active_sessions"`
	CurrentTool       string                  `json:"current_tool,omitempty"`
	CurrentToolArgs   string                  `json:"current_tool_args,omitempty"`
	ToolCount         int                     `json:"tool_count"`
	ConnectedFolders  []string                `json:"connected_folders"`
	MCPServers        []RemoteMCPServerStatus `json:"mcp_servers"`
	RecentEvents      []robin.ToolExecEvent   `json:"recent_events"`
	LastSyncUnix      int64                   `json:"last_sync_unix,omitempty"`
}

// RemoteController는 데몬의 원격 연결 제어 및 실시간 활동 구독 인터페이스다.
type RemoteController interface {
	RemoteStatus() RemoteStatusSnapshot
	SubscribeRemoteEvents() (<-chan robin.ToolExecEvent, func())
	SetRemoteEnabled(ctx context.Context, enabled bool) error
	SyncRegistration(ctx context.Context) error
	AddConnectedFolder(ctx context.Context, dir string) error
	RemoveConnectedFolder(ctx context.Context, dir string) error
}

// SetRemoteController는 데몬 원격 제어 컨트롤러를 로컬 IPC 서버에 주입한다.
func (s *Server) SetRemoteController(ctrl RemoteController) {
	s.remoteCtrl = ctrl
}

func (s *Server) fallbackRemoteStatus() RemoteStatusSnapshot {
	folders := append([]string{}, s.cfg.ConnectedFolders...)
	mcps := make([]RemoteMCPServerStatus, 0, len(s.cfg.MCPServers))
	for _, m := range s.cfg.MCPServers {
		mcps = append(mcps, RemoteMCPServerStatus{
			ServerID:    m.ServerID,
			DisplayName: m.DisplayName,
			Enabled:     m.Enabled,
		})
	}
	return RemoteStatusSnapshot{
		DeviceID:         s.cfg.DeviceID,
		DeviceName:       s.cfg.DeviceName,
		RemoteEnabled:    s.cfg.IsRemoteEnabled(),
		ActiveSessions:   []string{},
		ToolCount:        s.robinCli.ToolCount(),
		ConnectedFolders: folders,
		MCPServers:       mcps,
		RecentEvents:     []robin.ToolExecEvent{},
	}
}

func (s *Server) currentRemoteStatus() RemoteStatusSnapshot {
	if s.remoteCtrl != nil {
		return s.remoteCtrl.RemoteStatus()
	}
	return s.fallbackRemoteStatus()
}

func (s *Server) handleGetRemoteStatus(w http.ResponseWriter, r *http.Request) {
	writeJSON(w, http.StatusOK, s.currentRemoteStatus())
}

func (s *Server) handleToggleRemote(w http.ResponseWriter, r *http.Request) {
	if s.remoteCtrl == nil {
		writeError(w, http.StatusServiceUnavailable, fmt.Errorf("원격 제어 컨트롤러가 연결되지 않았습니다"))
		return
	}
	var req struct {
		Enabled bool `json:"enabled"`
	}
	if err := json.NewDecoder(r.Body).Decode(&req); err != nil {
		writeError(w, http.StatusBadRequest, err)
		return
	}
	if err := s.remoteCtrl.SetRemoteEnabled(r.Context(), req.Enabled); err != nil {
		writeError(w, http.StatusBadGateway, err)
		return
	}
	writeJSON(w, http.StatusOK, s.currentRemoteStatus())
}

func (s *Server) handleRegisterRemote(w http.ResponseWriter, r *http.Request) {
	if s.remoteCtrl == nil {
		writeError(w, http.StatusServiceUnavailable, fmt.Errorf("원격 제어 컨트롤러가 연결되지 않았습니다"))
		return
	}
	if err := s.remoteCtrl.SyncRegistration(r.Context()); err != nil {
		writeError(w, http.StatusBadGateway, err)
		return
	}
	writeJSON(w, http.StatusOK, s.currentRemoteStatus())
}

func (s *Server) handleAddRemoteFolder(w http.ResponseWriter, r *http.Request) {
	if s.remoteCtrl == nil {
		writeError(w, http.StatusServiceUnavailable, fmt.Errorf("원격 제어 컨트롤러가 연결되지 않았습니다"))
		return
	}
	var req struct {
		Path string `json:"path"`
	}
	if err := json.NewDecoder(r.Body).Decode(&req); err != nil {
		writeError(w, http.StatusBadRequest, err)
		return
	}
	if strings.TrimSpace(req.Path) == "" {
		writeError(w, http.StatusBadRequest, fmt.Errorf("추가할 폴더 경로가 비어 있습니다"))
		return
	}
	if err := s.remoteCtrl.AddConnectedFolder(r.Context(), req.Path); err != nil {
		writeError(w, http.StatusBadRequest, err)
		return
	}
	writeJSON(w, http.StatusOK, s.currentRemoteStatus())
}

func (s *Server) handleRemoveRemoteFolder(w http.ResponseWriter, r *http.Request) {
	if s.remoteCtrl == nil {
		writeError(w, http.StatusServiceUnavailable, fmt.Errorf("원격 제어 컨트롤러가 연결되지 않았습니다"))
		return
	}
	var req struct {
		Path string `json:"path"`
	}
	if err := json.NewDecoder(r.Body).Decode(&req); err != nil {
		writeError(w, http.StatusBadRequest, err)
		return
	}
	if strings.TrimSpace(req.Path) == "" {
		writeError(w, http.StatusBadRequest, fmt.Errorf("삭제할 폴더 경로가 비어 있습니다"))
		return
	}
	if err := s.remoteCtrl.RemoveConnectedFolder(r.Context(), req.Path); err != nil {
		writeError(w, http.StatusBadRequest, err)
		return
	}
	writeJSON(w, http.StatusOK, s.currentRemoteStatus())
}

func (s *Server) handleRemoteEvents(w http.ResponseWriter, r *http.Request) {
	flusher, ok := w.(http.Flusher)
	if !ok {
		writeError(w, http.StatusInternalServerError, fmt.Errorf("스트리밍 플러시를 지원하지 않습니다"))
		return
	}

	w.Header().Set("Content-Type", "text/event-stream")
	w.Header().Set("Cache-Control", "no-cache")
	w.Header().Set("Connection", "keep-alive")
	w.WriteHeader(http.StatusOK)

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

	// 최초 연결 시 현재 스냅샷 즉시 전송
	if err := writeSSE("status", s.currentRemoteStatus()); err != nil {
		return
	}

	if s.remoteCtrl == nil {
		return
	}
	ch, unsub := s.remoteCtrl.SubscribeRemoteEvents()
	defer unsub()

	keepalive := time.NewTicker(20 * time.Second)
	defer keepalive.Stop()

	for {
		select {
		case <-r.Context().Done():
			return
		case <-keepalive.C:
			if _, err := fmt.Fprint(w, ": keepalive\n\n"); err != nil {
				return
			}
			flusher.Flush()
		case ev, ok := <-ch:
			if !ok {
				return
			}
			if err := writeSSE("tool", ev); err != nil {
				return
			}
			if err := writeSSE("status", s.currentRemoteStatus()); err != nil {
				return
			}
		}
	}
}
