package daemon

import (
	"context"
	"fmt"
	"os"
	"path/filepath"
	"sort"
	"strings"
	"time"

	"github.com/sam/gemini-remote-linux/internal/localapi"
	"github.com/sam/gemini-remote-linux/internal/robin"
)

const maxRecentToolEvents = 30

// RemoteStatus는 현재 이 PC 원격 제어 데몬의 실시간 상태 스냅샷을 반환한다.
func (d *Daemon) RemoteStatus() localapi.RemoteStatusSnapshot {
	d.sessionMu.Lock()
	sessions := make([]string, 0, len(d.activeSessions))
	for rk := range d.activeSessions {
		sessions = append(sessions, rk)
	}
	d.sessionMu.Unlock()
	sort.Strings(sessions)

	d.statusMu.RLock()
	curTool := d.currentTool
	curArgs := d.currentToolArgs
	lastSync := d.lastSyncUnix
	events := make([]robin.ToolExecEvent, len(d.recentEvents))
	copy(events, d.recentEvents)
	d.statusMu.RUnlock()

	enabled := d.cfg.IsRemoteEnabled()
	folders := d.cfg.CloneConnectedFolders()
	cfgMCPs := d.cfg.CloneMCPServers()
	mcps := make([]localapi.RemoteMCPServerStatus, 0, len(cfgMCPs))
	for _, m := range cfgMCPs {
		mcps = append(mcps, localapi.RemoteMCPServerStatus{
			ServerID:    m.ServerID,
			DisplayName: m.DisplayName,
			Enabled:     m.Enabled,
		})
	}

	punctualConnected := false
	if d.watcher != nil {
		punctualConnected = d.watcher.IsConnected()
	}

	return localapi.RemoteStatusSnapshot{
		DeviceID:          d.cfg.DeviceID,
		DeviceName:        d.cfg.DeviceName,
		RemoteEnabled:     enabled,
		PunctualConnected: punctualConnected,
		ActiveSessions:    sessions,
		CurrentTool:       curTool,
		CurrentToolArgs:   curArgs,
		ToolCount:         d.robinCli.ToolCount(),
		ConnectedFolders:  folders,
		MCPServers:        mcps,
		RecentEvents:      events,
		LastSyncUnix:      lastSync,
	}
}

// SubscribeRemoteEvents는 원격 도구 실행 및 연결 상태 변경 이벤트 채널을 구독한다.
func (d *Daemon) SubscribeRemoteEvents() (<-chan robin.ToolExecEvent, func()) {
	ch := make(chan robin.ToolExecEvent, 32)
	d.statusMu.Lock()
	if d.subscribers == nil {
		d.subscribers = make(map[chan robin.ToolExecEvent]struct{})
	}
	d.subscribers[ch] = struct{}{}
	d.statusMu.Unlock()

	unsub := func() {
		d.statusMu.Lock()
		delete(d.subscribers, ch)
		d.statusMu.Unlock()
	}
	return ch, unsub
}

func (d *Daemon) recordToolEvent(ev robin.ToolExecEvent) {
	if ev.TimestampUnix == 0 {
		ev.TimestampUnix = time.Now().Unix()
	}

	d.statusMu.Lock()
	switch ev.Phase {
	case "started":
		d.currentTool = ev.ToolName
		d.currentToolArgs = ev.ArgsBrief
	case "finished", "error", "session_ended", "turn_completed":
		if ev.Phase == "session_ended" || ev.Phase == "turn_completed" || d.currentTool == ev.ToolName {
			d.currentTool = ""
			d.currentToolArgs = ""
		}
	}
	// 상태 알림이나 세션 수명 이벤트가 아닌 실제 원격 도구 호출만 최근 이력에 보관
	if ev.Phase != "status_changed" && ev.ToolName != "remote_session" {
		d.recentEvents = append(d.recentEvents, ev)
		if len(d.recentEvents) > maxRecentToolEvents {
			d.recentEvents = d.recentEvents[len(d.recentEvents)-maxRecentToolEvents:]
		}
	}
	for ch := range d.subscribers {
		select {
		case ch <- ev:
		default:
		}
	}
	d.statusMu.Unlock()
}

func (d *Daemon) notifyStatusChanged(toolName string) {
	d.recordToolEvent(robin.ToolExecEvent{
		ToolName:      toolName,
		Phase:         "status_changed",
		TimestampUnix: time.Now().Unix(),
	})
}

func (d *Daemon) notifyConversationUpdated(convID string) {
	d.recordToolEvent(robin.ToolExecEvent{
		SessionID:     convID,
		ToolName:      "conversations_updated",
		Phase:         "status_changed",
		TimestampUnix: time.Now().Unix(),
	})
}

func (d *Daemon) markSynced() {
	d.statusMu.Lock()
	d.lastSyncUnix = time.Now().Unix()
	d.statusMu.Unlock()
}

func (d *Daemon) cancelAllSessions() {
	d.sessionMu.Lock()
	for rk, entry := range d.activeSessions {
		if entry != nil && entry.cancel != nil {
			entry.cancel()
		}
		delete(d.activeSessions, rk)
	}
	d.sessionMu.Unlock()

	d.statusMu.Lock()
	d.currentTool = ""
	d.currentToolArgs = ""
	d.statusMu.Unlock()
}

// SetRemoteEnabled는 이 PC의 원격 제어 수신 활성화 상태를 전환하고 서버에 즉시 반영한다.
func (d *Daemon) SetRemoteEnabled(ctx context.Context, enabled bool) error {
	d.cfgMu.Lock()
	d.cfg.SetRemoteEnabled(enabled)
	saveErr := d.cfg.Save()
	d.cfgMu.Unlock()
	if saveErr != nil {
		return fmt.Errorf("설정 저장 실패: %w", saveErr)
	}

	if enabled {
		if _, err := d.robinCli.RegisterDevice(ctx); err != nil {
			return fmt.Errorf("기기 등록 실패: %w", err)
		}
		_ = d.robinCli.ReportClientPresence(ctx)
		d.markSynced()
		fmt.Printf("[daemon] 원격 제어 활성화 및 서버 등록 완료\n")
	} else {
		d.cancelAllSessions()
		if err := d.robinCli.UnregisterDevice(ctx); err != nil {
			return fmt.Errorf("기기 등록 해제 실패: %w", err)
		}
		d.markSynced()
		fmt.Printf("[daemon] 원격 제어 비활성화 및 서버 등록 해제 완료\n")
	}
	d.notifyStatusChanged("remote_toggle")
	return nil
}

// SyncRegistration은 현재 설정(허용 폴더·도구·MCP)을 서버에 즉시 재등록한다.
func (d *Daemon) SyncRegistration(ctx context.Context) error {
	d.cfgMu.RLock()
	enabled := d.cfg.IsRemoteEnabled()
	d.cfgMu.RUnlock()
	if !enabled {
		return fmt.Errorf("원격 제어가 꺼져 있습니다. 먼저 원격 연결을 켜세요")
	}
	if _, err := d.robinCli.RegisterDevice(ctx); err != nil {
		return fmt.Errorf("RegisterDevice 실패: %w", err)
	}
	_ = d.robinCli.ReportClientPresence(ctx)
	d.markSynced()
	d.notifyStatusChanged("register_sync")
	return nil
}

func normalizeFolderPath(raw string) (string, error) {
	p := strings.TrimSpace(raw)
	if p == "" {
		return "", fmt.Errorf("폴더 경로가 비어 있습니다")
	}
	if p == "~" || strings.HasPrefix(p, "~/") {
		if home, err := os.UserHomeDir(); err == nil {
			if p == "~" {
				p = home
			} else {
				p = filepath.Join(home, p[2:])
			}
		}
	}
	abs, err := filepath.Abs(p)
	if err != nil {
		return "", fmt.Errorf("잘못된 폴더 경로: %w", err)
	}
	return filepath.Clean(abs), nil
}

// AddConnectedFolder는 원격 허용 폴더를 추가·저장하고 원격 활성화 시 서버에 즉시 동기화한다.
func (d *Daemon) AddConnectedFolder(ctx context.Context, dir string) error {
	clean, err := normalizeFolderPath(dir)
	if err != nil {
		return err
	}
	st, err := os.Stat(clean)
	if err != nil {
		return fmt.Errorf("폴더를 찾을 수 없습니다 (%s): %w", clean, err)
	}
	if !st.IsDir() {
		return fmt.Errorf("디렉터리가 아닙니다: %s", clean)
	}

	d.cfgMu.Lock()
	d.cfg.AddConnectedFolder(clean)
	enabled := d.cfg.IsRemoteEnabled()
	saveErr := d.cfg.Save()
	d.cfgMu.Unlock()
	if saveErr != nil {
		return fmt.Errorf("설정 저장 실패: %w", saveErr)
	}

	if enabled {
		if _, err := d.robinCli.RegisterDevice(ctx); err != nil {
			fmt.Printf("[daemon] 폴더 추가 후 RegisterDevice 경고: %v\n", err)
		} else {
			d.markSynced()
		}
	}
	d.notifyStatusChanged("folders_changed")
	return nil
}

// RemoveConnectedFolder는 원격 허용 폴더를 제거·저장하고 원격 활성화 시 서버에 즉시 동기화한다.
func (d *Daemon) RemoveConnectedFolder(ctx context.Context, dir string) error {
	clean, err := normalizeFolderPath(dir)
	if err != nil {
		return err
	}

	d.cfgMu.Lock()
	removed := d.cfg.RemoveConnectedFolder(clean, dir)
	if !removed {
		d.cfgMu.Unlock()
		return fmt.Errorf("목록에 없는 폴더입니다: %s", clean)
	}
	enabled := d.cfg.IsRemoteEnabled()
	saveErr := d.cfg.Save()
	d.cfgMu.Unlock()
	if saveErr != nil {
		return fmt.Errorf("설정 저장 실패: %w", saveErr)
	}

	if enabled {
		if _, err := d.robinCli.RegisterDevice(ctx); err != nil {
			fmt.Printf("[daemon] 폴더 삭제 후 RegisterDevice 경고: %v\n", err)
		} else {
			d.markSynced()
		}
	}
	d.notifyStatusChanged("folders_changed")
	return nil
}
