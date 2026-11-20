package daemon

import (
	"context"
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"sync"
	"time"

	"github.com/sam/gemini-remote-linux/internal/auth"
	"github.com/sam/gemini-remote-linux/internal/config"
	"github.com/sam/gemini-remote-linux/internal/localapi"
	"github.com/sam/gemini-remote-linux/internal/mcp"
	"github.com/sam/gemini-remote-linux/internal/proto"
	"github.com/sam/gemini-remote-linux/internal/punctual"
	"github.com/sam/gemini-remote-linux/internal/robin"
	"github.com/sam/gemini-remote-linux/internal/scotty"
	"github.com/sam/gemini-remote-linux/internal/tools"
)

// Daemon은 RegisterDevice, 주기적 ReportClientPresence, Punctual WakeUp 수신, ExecuteRemoteTools 스트림을 총괄한다.
type Daemon struct {
	cfg           *config.Config
	authMgr       *auth.Manager
	robinCli      *robin.Client
	watcher       *punctual.Watcher
	sessionRunner func(ctx context.Context, routingKey, routingToken string, hooks robin.SessionHooks) error

	cfgMu          sync.RWMutex
	sessionMu      sync.Mutex
	activeSessions map[string]*sessionEntry
	lastHintProbe  map[string]time.Time

	statusMu        sync.RWMutex
	currentTool     string
	currentToolArgs string
	lastSyncUnix    int64
	recentEvents    []robin.ToolExecEvent
	subscribers     map[chan robin.ToolExecEvent]struct{}
}

type sessionEntry struct {
	id            uint64
	cancel        context.CancelFunc
	startedAt     time.Time
	routingToken  string
	isIdle        bool
	hadToolCall   bool
	inToolCall    bool
	pendingWakeUp bool
	pendingToken  string
}

var nextSessionSeq uint64

func New(cfg *config.Config, authMgr *auth.Manager) *Daemon {
	uploader := scotty.NewUploader()
	mcpReg := mcp.NewRegistry(cfg)
	executor := tools.NewExecutor(cfg, authMgr, uploader, mcpReg)
	robinCli := robin.NewClient(cfg, authMgr, executor, mcpReg)
	watcher := punctual.NewWatcher(authMgr, cfg.DeviceID)
	d := &Daemon{
		cfg:            cfg,
		authMgr:        authMgr,
		robinCli:       robinCli,
		watcher:        watcher,
		sessionRunner:  robinCli.ExecuteRemoteToolsSessionWithHooks,
		activeSessions: make(map[string]*sessionEntry),
		lastHintProbe:  make(map[string]time.Time),
		subscribers:    make(map[chan robin.ToolExecEvent]struct{}),
	}
	executor.SetOnFoldersChanged(func() {
		ctx, cancel := context.WithTimeout(context.Background(), 15*time.Second)
		defer cancel()
		if _, err := robinCli.RegisterDevice(ctx); err != nil {
			fmt.Printf("[daemon] 폴더 변경 후 RegisterDevice 경고: %v\n", err)
		} else {
			d.markSynced()
			fmt.Printf("[daemon] 연결 폴더 변경사항 서버 등록 완료 (%d개)\n", len(cfg.CloneConnectedFolders()))
		}
		d.notifyStatusChanged("folders_changed")
	})
	watcher.SetOnConnectionChange(func(connected bool) {
		d.notifyStatusChanged("punctual_connection")
	})
	return d
}

func (d *Daemon) RobinClient() *robin.Client {
	return d.robinCli
}

func (d *Daemon) runSession(ctx context.Context, routingKey, routingToken string, hooks robin.SessionHooks) error {
	if d.sessionRunner != nil {
		return d.sessionRunner(ctx, routingKey, routingToken, hooks)
	}
	return d.robinCli.ExecuteRemoteToolsSessionWithHooks(ctx, routingKey, routingToken, hooks)
}

// Serve는 데몬 메인 루프를 실행한다.
func (d *Daemon) Serve(ctx context.Context) error {
	fmt.Printf("[daemon] 리눅스 원격 제어 데몬 시작 (name=%s)\n", d.cfg.DeviceName)

	if d.cfg.IsRemoteEnabled() {
		res, err := d.robinCli.RegisterDevice(ctx)
		if err != nil {
			return fmt.Errorf("초기 RegisterDevice 실패: %w", err)
		}
		d.markSynced()
		fmt.Printf("[daemon] 기기 등록 완료 (surface=%d, delivery_address=%s)\n", res.TargetSurface, res.DeliveryAddress)
		_ = d.robinCli.ReportClientPresence(ctx)
	} else {
		fmt.Printf("[daemon] 원격 제어 비활성화 상태로 시작 (기기 등록 대기)\n")
	}

	// 로컬 IPC 소켓 HTTP/SSE 서버 기동 (네이티브 UI 및 CLI 연동용)
	if sockPath, err := config.SocketPath(); err == nil {
		apiSrv := localapi.NewServer(d.cfg, d.robinCli, sockPath)
		apiSrv.SetRemoteController(d)
		go func() {
			if err := apiSrv.Serve(ctx); err != nil && ctx.Err() == nil {
				fmt.Printf("[daemon] 로컬 IPC 서버 경고: %v\n", err)
			}
		}()
	} else {
		fmt.Printf("[daemon] 로컬 소켓 경로 확인 실패: %v\n", err)
	}

	// 주기적 Presence 보고 및 재등록 루프
	go func() {
		presenceTicker := time.NewTicker(2 * time.Minute)
		registerTicker := time.NewTicker(30 * time.Minute)
		defer presenceTicker.Stop()
		defer registerTicker.Stop()

		for {
			select {
			case <-ctx.Done():
				return
			case <-presenceTicker.C:
				d.cfgMu.RLock()
				enabled := d.cfg.IsRemoteEnabled()
				d.cfgMu.RUnlock()
				if !enabled {
					continue
				}
				if err := d.robinCli.ReportClientPresence(ctx); err != nil {
					fmt.Printf("[daemon] ReportClientPresence 경고: %v\n", err)
				}
			case <-registerTicker.C:
				d.cfgMu.RLock()
				enabled := d.cfg.IsRemoteEnabled()
				d.cfgMu.RUnlock()
				if !enabled {
					continue
				}
				if _, err := d.robinCli.RegisterDevice(ctx); err != nil {
					fmt.Printf("[daemon] 주기적 RegisterDevice 경고: %v\n", err)
				} else {
					d.markSynced()
				}
			}
		}
	}()

	// Punctual WebChannel 실시간 감시 루프
	return d.watcher.Run(ctx, d.handleWakeUpEvent)
}

func (d *Daemon) handleWakeUpEvent(ctx context.Context, ev *proto.WakeUpEvent) {
	if ev == nil {
		return
	}
	if ev.RefreshDeviceStatus {
		if d.robinCli != nil {
			d.robinCli.InvalidateDevicesCache()
			go func() {
				syncCtx, cancel := context.WithTimeout(ctx, 10*time.Second)
				defer cancel()
				_, _ = d.robinCli.ListDevices(syncCtx)
				d.notifyStatusChanged("device_status_sync")
			}()
		} else {
			d.notifyStatusChanged("device_status_sync")
		}
		if ev.SessionRoutingKey == "" {
			return
		}
	}
	if ev.SessionRoutingKey == "" {
		return
	}
	enabled := d.cfg.IsRemoteEnabled()
	if !enabled {
		return
	}
	if ev.IsClientSyncHint {
		sessionCtx, entry, ok := d.tryStartSessionFromHint(ctx, ev.SessionRoutingKey)
		if !ok {
			return
		}
		defer d.releaseSession(ev.SessionRoutingKey, entry.id)
		fmt.Printf("[daemon] ClientSync 대화방 힌트로 세션 복구 시도 (routing_key=%s, seq=%d)\n", ev.SessionRoutingKey, entry.id)
		d.runSessionLoop(ctx, sessionCtx, ev.SessionRoutingKey, entry, "")
		return
	}

	sessionCtx, entry, ok := d.acquireWakeUpSession(ctx, ev.SessionRoutingKey, ev.RoutingToken)
	if !ok {
		return
	}
	defer d.releaseSession(ev.SessionRoutingKey, entry.id)
	fmt.Printf("[daemon] WakeUp 수신 -> 원격 도구 세션 시작 (routing_key=%s, seq=%d)\n", ev.SessionRoutingKey, entry.id)
	d.runSessionLoop(ctx, sessionCtx, ev.SessionRoutingKey, entry, ev.RoutingToken)
}

func (d *Daemon) runSessionLoop(parentCtx, sessionCtx context.Context, routingKey string, entry *sessionEntry, initialToken string) {
	hooks := robin.SessionHooks{
		OnToolCallStart: func() {
			d.sessionMu.Lock()
			if !entry.hadToolCall {
				// 초기 핸드셰이크 중 보류된 동일 턴 WakeUp은 첫 도구 호출 시작으로 이미 처리되었으므로 소거한다.
				entry.pendingWakeUp = false
				entry.pendingToken = ""
			}
			entry.hadToolCall = true
			entry.inToolCall = true
			entry.isIdle = false
			d.sessionMu.Unlock()
		},
		OnToolCallEnd: func() {
			d.sessionMu.Lock()
			entry.inToolCall = false
			d.sessionMu.Unlock()
		},
		OnIdle: func(idle bool) {
			d.sessionMu.Lock()
			entry.isIdle = idle
			d.sessionMu.Unlock()
		},
		OnToolEvent: func(ev robin.ToolExecEvent) {
			d.recordToolEvent(ev)
		},
	}

	currentToken := initialToken
	for {
		err := d.runSession(sessionCtx, routingKey, currentToken, hooks)
		d.sessionMu.Lock()
		hadToolCall := entry.hadToolCall
		resumePending := entry.pendingWakeUp && parentCtx.Err() == nil && sessionCtx.Err() == nil
		nextToken := entry.pendingToken
		if resumePending {
			entry.pendingWakeUp = false
			entry.pendingToken = ""
			entry.startedAt = time.Now()
			if nextToken != "" {
				entry.routingToken = nextToken
			}
		}
		d.sessionMu.Unlock()

		if err != nil {
			if currentToken == "" && !hadToolCall {
				fmt.Printf("[daemon] 대화방 힌트 세션 종료 (routing_key=%s): %v\n", routingKey, err)
			} else {
				fmt.Printf("[daemon] 원격 도구 세션 오류 (routing_key=%s): %v\n", routingKey, err)
			}
		}
		if !resumePending {
			return
		}
		fmt.Printf("[daemon] 보류된 후속 WakeUp 승계 실행 (routing_key=%s, seq=%d)\n", routingKey, entry.id)
		currentToken = nextToken
	}
}

func (d *Daemon) tryStartSessionFromHint(parent context.Context, routingKey string) (context.Context, *sessionEntry, bool) {
	d.sessionMu.Lock()
	defer d.sessionMu.Unlock()
	if prev, exists := d.activeSessions[routingKey]; exists && prev != nil {
		return nil, nil, false
	}
	now := time.Now()
	if len(d.lastHintProbe) > 64 {
		for k, ts := range d.lastHintProbe {
			if now.Sub(ts) > time.Minute {
				delete(d.lastHintProbe, k)
			}
		}
	}
	if last, ok := d.lastHintProbe[routingKey]; ok && now.Sub(last) < 3*time.Second {
		return nil, nil, false
	}
	d.lastHintProbe[routingKey] = now
	nextSessionSeq++
	seq := nextSessionSeq
	sessionCtx, cancel := context.WithCancel(parent)
	entry := &sessionEntry{
		id:        seq,
		cancel:    cancel,
		startedAt: time.Now(),
	}
	d.activeSessions[routingKey] = entry
	return sessionCtx, entry, true
}

func (d *Daemon) acquireWakeUpSession(parent context.Context, routingKey, routingToken string) (context.Context, *sessionEntry, bool) {
	d.sessionMu.Lock()
	defer d.sessionMu.Unlock()
	if prev, exists := d.activeSessions[routingKey]; exists && prev != nil {
		// 멀티턴 유휴 대기 중이거나 15초 이상 응답 없는 무토큰 세션일 때만 기존 세션을 교체한다.
		// 초기 핸드셰이크/도구 실행 중인 세션을 덮어쓰면 서버가 첫 스트림으로 보낸 첫 턴 도구 호출이 유실된다.
		shouldReplace := prev.isIdle || (!prev.inToolCall && !prev.hadToolCall && prev.routingToken == "" && routingToken != "" && time.Since(prev.startedAt) >= 15*time.Second)
		if !shouldReplace {
			prev.pendingWakeUp = true
			if routingToken != "" {
				prev.pendingToken = routingToken
			}
			fmt.Printf("[daemon] 활성/핸드셰이크 세션 보호 -> 후속 WakeUp 보류 (routing_key=%s, seq=%d, idle=%v)\n", routingKey, prev.id, prev.isIdle)
			return nil, nil, false
		}
		fmt.Printf("[daemon] 유휴 대기 스트림 교체 -> 새 WakeUp 세션 시작 (routing_key=%s, prev_seq=%d)\n", routingKey, prev.id)
		prev.cancel()
	}
	nextSessionSeq++
	seq := nextSessionSeq
	sessionCtx, cancel := context.WithCancel(parent)
	entry := &sessionEntry{
		id:           seq,
		cancel:       cancel,
		startedAt:    time.Now(),
		routingToken: routingToken,
	}
	d.activeSessions[routingKey] = entry
	return sessionCtx, entry, true
}

func (d *Daemon) releaseSession(routingKey string, seq uint64) {
	released := false
	d.sessionMu.Lock()
	if cur, exists := d.activeSessions[routingKey]; exists && cur.id == seq {
		cur.cancel()
		delete(d.activeSessions, routingKey)
		released = true
	}
	d.sessionMu.Unlock()
	if released {
		d.recordToolEvent(robin.ToolExecEvent{
			SessionID:     routingKey,
			ToolName:      "remote_session",
			Phase:         "session_ended",
			TimestampUnix: time.Now().Unix(),
		})
	}
}

// InstallSystemdUserService는 ~/.config/systemd/user/gemini-remote.service 유닛을 생성하고 활성화한다.
func InstallSystemdUserService() (string, error) {
	exePath, err := os.Executable()
	if err != nil {
		return "", err
	}
	exePath, err = filepath.EvalSymlinks(exePath)
	if err != nil {
		return "", err
	}

	home, err := os.UserHomeDir()
	if err != nil {
		return "", err
	}
	unitDir := filepath.Join(home, ".config", "systemd", "user")
	if err := os.MkdirAll(unitDir, 0o755); err != nil {
		return "", err
	}
	unitPath := filepath.Join(unitDir, "gemini-remote.service")

	unitContent := fmt.Sprintf(`[Unit]
Description=Gemini Remote Control Daemon for Linux (macOS Gemini.app compatible)
After=network-online.target graphical-session.target
Wants=network-online.target

[Service]
Type=simple
ExecStart=%s serve
Restart=always
RestartSec=5
Environment=PATH=%s

[Install]
WantedBy=default.target
`, exePath, os.Getenv("PATH"))

	if err := os.WriteFile(unitPath, []byte(unitContent), 0o644); err != nil {
		return "", err
	}

	_ = exec.Command("systemctl", "--user", "daemon-reload").Run()
	if out, err := exec.Command("systemctl", "--user", "enable", "--now", "gemini-remote.service").CombinedOutput(); err != nil {
		return unitPath, fmt.Errorf("유닛 파일 생성 완료(%s), systemctl 활성화 경고: %v (%s)", unitPath, err, string(out))
	}
	return unitPath, nil
}

// UninstallSystemdUserService는 systemd 사용자 서비스를 중지하고 제거한다.
func UninstallSystemdUserService() error {
	_ = exec.Command("systemctl", "--user", "disable", "--now", "gemini-remote.service").Run()
	home, err := os.UserHomeDir()
	if err != nil {
		return err
	}
	unitPath := filepath.Join(home, ".config", "systemd", "user", "gemini-remote.service")
	_ = os.Remove(unitPath)
	_ = exec.Command("systemctl", "--user", "daemon-reload").Run()
	return nil
}

// StatusSystemdUserService는 systemd 사용자 서비스 상태를 반환한다.
func StatusSystemdUserService() (string, error) {
	out, err := exec.Command("systemctl", "--user", "status", "--no-pager", "gemini-remote.service").CombinedOutput()
	return string(out), err
}
