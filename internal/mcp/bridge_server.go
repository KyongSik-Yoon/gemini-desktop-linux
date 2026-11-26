package mcp

import (
	"bufio"
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"math"
	"net/http"
	"strings"
	"sync"
	"sync/atomic"
	"time"

	"github.com/sam/gemini-remote-linux/internal/config"
	"github.com/sam/gemini-remote-linux/internal/proto"
)

const (
	bridgeDefaultProtocolVersion = "2024-11-05"
	bridgeServerName             = "gemini-remote"
	bridgeServerVersion          = "1.3.0"

	// Antigravity 등 MCP 클라이언트의 기본 3분(180초) 도구 호출 타임아웃보다 여유 있게 반환하고
	// 중간 진행 상황을 주기적으로 중계할 수 있도록 기본 대기 시간을 60초로 둔다.
	bridgeDefaultWaitDuration        = 60 * time.Second
	bridgeMaxWaitDuration            = 150 * time.Second
	bridgeMaxCompletedJobs           = 64
	bridgeDefaultRecoveryInterval    = 10 * time.Second
	bridgeSSEFallbackInterval        = 60 * time.Second
	bridgeDefaultRecoveryIdleTimeout = 15 * time.Minute
	bridgeDefaultRecoveryMaxWait     = 2 * time.Hour
	bridgeDefaultRecoverySettleDelay = 6 * time.Second
	bridgeProgressThrottle           = 1500 * time.Millisecond
)

// BridgeBackend abstracts the RobinService operations used by the MCP bridge server.
type BridgeBackend interface {
	GetUserStatus(ctx context.Context) (*proto.UserStatusResult, error)
	GetUsageInfo(ctx context.Context) (*proto.UsageInfo, error)
	ListDevices(ctx context.Context) ([]proto.RegisteredDeviceSummary, error)
	ListConversationTurns(ctx context.Context, conversationID string, pageSize int32, pageToken string) (*proto.ListConversationTurnsResult, error)
	StreamQuery(ctx context.Context, params proto.ChatQueryParams, onChunk func(proto.ChatStreamChunk) error) (*proto.ChatStreamChunk, error)
}

// ResolveModel matches a model by ID, display title, or case-insensitive substring.
func ResolveModel(models []proto.BardModeOption, query string) (proto.BardModeOption, bool) {
	q := strings.TrimSpace(query)
	if q == "" {
		return proto.BardModeOption{}, false
	}
	lowerQ := strings.ToLower(q)

	for _, m := range models {
		if strings.EqualFold(m.ID, q) {
			return m, true
		}
	}
	for _, m := range models {
		if strings.EqualFold(m.TitleForPicker, q) || strings.EqualFold(m.Title, q) || strings.EqualFold(m.ModeName, q) {
			return m, true
		}
	}
	for _, m := range models {
		if strings.Contains(strings.ToLower(m.ID), lowerQ) ||
			strings.Contains(strings.ToLower(m.TitleForPicker), lowerQ) ||
			strings.Contains(strings.ToLower(m.Title), lowerQ) {
			return m, true
		}
	}
	return proto.BardModeOption{}, false
}

type bridgeJobStatus string

const (
	bridgeJobRunning   bridgeJobStatus = "running"
	bridgeJobCompleted bridgeJobStatus = "completed"
	bridgeJobFailed    bridgeJobStatus = "failed"
	bridgeJobCancelled bridgeJobStatus = "cancelled"
)

type progressUpdate struct {
	Progress int
	Message  string
}

type daemonToolEvent struct {
	SessionID string `json:"session_id"`
	CallID    string `json:"call_id,omitempty"`
	ToolName  string `json:"tool_name"`
	ArgsBrief string `json:"args_brief,omitempty"`
	Phase     string `json:"phase"`
	Error     string `json:"error,omitempty"`
}

type bridgeJob struct {
	mu              sync.Mutex
	id              string
	startedAt       time.Time
	lastActivityAt  time.Time
	finishedAt      time.Time
	status          bridgeJobStatus
	phase           string
	agentTask       bool
	conversationID  string
	modelAttr       string
	targetDevice    string
	statusText      string
	liveTool        string
	thoughts        strings.Builder
	streamedText    strings.Builder
	seenTools       map[string]int
	activities      []proto.RemoteToolActivity
	lastProgressMsg string
	progressSeq     uint64
	progressSubs    map[uint64]chan progressUpdate
	finalResult     bridgeToolCallResult
	errText         string
	done            chan struct{}
	cancel          context.CancelFunc
}

func formatActivity(a proto.RemoteToolActivity) string {
	detail := strings.TrimSpace(a.Explanation)
	if detail == "" {
		detail = strings.TrimSpace(a.Summary)
	}
	if detail != "" && a.Name != "" {
		return fmt.Sprintf("%s(%s)", a.Name, detail)
	}
	if a.Name != "" {
		return a.Name
	}
	return detail
}

func tailRunes(s string, maxRunes int) string {
	s = strings.TrimSpace(s)
	if s == "" || maxRunes <= 0 {
		return ""
	}
	r := []rune(s)
	if len(r) <= maxRunes {
		return s
	}
	return "..." + strings.TrimSpace(string(r[len(r)-maxRunes:]))
}

func (j *bridgeJob) subscribeProgress() (uint64, <-chan progressUpdate) {
	j.mu.Lock()
	defer j.mu.Unlock()
	if j.progressSubs == nil {
		j.progressSubs = make(map[uint64]chan progressUpdate)
	}
	j.progressSeq++
	id := j.progressSeq
	ch := make(chan progressUpdate, 1)
	j.progressSubs[id] = ch
	return id, ch
}

func (j *bridgeJob) unsubscribeProgress(id uint64) {
	j.mu.Lock()
	delete(j.progressSubs, id)
	j.mu.Unlock()
}

func (j *bridgeJob) emitProgressLocked(msg string) {
	msg = strings.TrimSpace(msg)
	if msg == "" || msg == j.lastProgressMsg {
		return
	}
	j.lastProgressMsg = msg
	upd := progressUpdate{
		Progress: len(j.activities),
		Message:  msg,
	}
	for _, ch := range j.progressSubs {
		select {
		case ch <- upd:
		default:
			select {
			case <-ch:
			default:
			}
			select {
			case ch <- upd:
			default:
			}
		}
	}
}

func (j *bridgeJob) appendActivitiesLocked(acts []proto.RemoteToolActivity) bool {
	if j.seenTools == nil {
		j.seenTools = make(map[string]int)
	}
	changed := false
	for _, a := range acts {
		if a.Name == "" && a.Explanation == "" && a.Summary == "" {
			continue
		}
		key := a.ID
		if key == "" {
			key = a.Name + "|" + a.Summary + "|" + a.Explanation
		}
		if idx, ok := j.seenTools[key]; ok {
			prev := j.activities[idx]
			if (prev.Explanation == "" && a.Explanation != "") || (prev.Summary == "" && a.Summary != "") {
				if a.Explanation != "" {
					j.activities[idx].Explanation = a.Explanation
				}
				if a.Summary != "" {
					j.activities[idx].Summary = a.Summary
				}
				changed = true
			}
			continue
		}
		j.seenTools[key] = len(j.activities)
		j.activities = append(j.activities, a)
		changed = true
	}
	if changed {
		j.lastActivityAt = time.Now()
		if len(j.activities) > 0 {
			j.emitProgressLocked(formatActivity(j.activities[len(j.activities)-1]))
		}
	}
	return changed
}

func (j *bridgeJob) recordDaemonEventLocked(ev daemonToolEvent) {
	now := time.Now()
	switch ev.Phase {
	case "started":
		if ev.ToolName == "" || ev.ToolName == "list_mcp" || ev.ToolName == "remote_session" {
			return
		}
		label := ev.ToolName
		if ev.ArgsBrief != "" {
			label = fmt.Sprintf("%s: %s", ev.ToolName, ev.ArgsBrief)
		}
		j.liveTool = label
		j.lastActivityAt = now
		j.emitProgressLocked("실행 중: " + label)
	case "finished", "error":
		if ev.ToolName == "" || ev.ToolName == "list_mcp" || ev.ToolName == "remote_session" {
			return
		}
		j.liveTool = ""
		j.lastActivityAt = now
		if ev.Phase == "error" && ev.Error != "" {
			j.emitProgressLocked(fmt.Sprintf("도구 오류(%s): %s", ev.ToolName, ev.Error))
		}
	case "turn_completed":
		j.liveTool = ""
		j.lastActivityAt = now
		j.emitProgressLocked("도구 실행 완료, 최종 응답 정리 중")
	case "session_ended":
		j.liveTool = ""
		j.lastActivityAt = now
	}
}

func (j *bridgeJob) snapshotRunning(now time.Time) bridgeToolCallResult {
	j.mu.Lock()
	defer j.mu.Unlock()

	elapsed := int(now.Sub(j.startedAt).Seconds())
	if elapsed < 0 {
		elapsed = 0
	}
	lastActSec := 0
	if !j.lastActivityAt.IsZero() {
		lastActSec = int(now.Sub(j.lastActivityAt).Seconds())
		if lastActSec < 0 {
			lastActSec = 0
		}
	}
	recent := make([]string, 0, min(len(j.activities), 5))
	startIdx := 0
	if len(j.activities) > 5 {
		startIdx = len(j.activities) - 5
	}
	for _, a := range j.activities[startIdx:] {
		if formatted := formatActivity(a); formatted != "" {
			recent = append(recent, formatted)
		}
	}

	payload := map[string]any{
		"status":                    string(bridgeJobRunning),
		"job_id":                    j.id,
		"phase":                     j.phase,
		"elapsed_seconds":           elapsed,
		"last_activity_seconds_ago": lastActSec,
		"partial_chars":             j.streamedText.Len(),
		"tools_count":               len(j.activities),
		"message":                   fmt.Sprintf("작업이 백그라운드에서 계속 실행 중입니다. 완료될 때까지 gemini_remote_wait(job_id=%q)를 호출해 이어서 대기하세요.", j.id),
	}
	if j.conversationID != "" {
		payload["conversation_id"] = j.conversationID
	}
	if j.modelAttr != "" {
		payload["model"] = j.modelAttr
	}
	if j.targetDevice != "" && j.agentTask {
		payload["device"] = j.targetDevice
	}
	if j.statusText != "" {
		payload["status_text"] = j.statusText
	}
	if j.liveTool != "" {
		payload["live_tool"] = j.liveTool
	}
	if thoughtsTail := tailRunes(j.thoughts.String(), 400); thoughtsTail != "" {
		payload["thoughts_tail"] = thoughtsTail
	}
	if len(recent) > 0 {
		payload["recent_tools"] = recent
	}
	b, _ := json.MarshalIndent(payload, "", "  ")
	return bridgeTextResult(string(b))
}

// BridgeServer is a stdio JSON-RPC 2.0 MCP server delegating to gemini-remote.
type BridgeServer struct {
	backend              BridgeBackend
	loadConfig           func() (*config.Config, error)
	writeMu              sync.Mutex
	logLevel             atomic.Int32 // 0=disabled, 1=debug, 2=info, 3=notice, 4=warning, 5=error
	defaultWait          time.Duration
	recoveryPollInterval time.Duration
	recoveryIdleTimeout  time.Duration
	recoveryMaxWait      time.Duration
	recoverySettleDelay  time.Duration
	daemonSocketPath     string
	daemonHTTPMu         sync.Mutex
	daemonHTTPPath       string
	daemonHTTP           *http.Client
	subscribeHints       func(ctx context.Context, convID string) (<-chan struct{}, func())

	jobsMu   sync.Mutex
	jobs     map[string]*bridgeJob
	jobOrder []string
	jobSeq   atomic.Uint64
}

// NewBridgeServer creates an MCP BridgeServer backed by the given BridgeBackend and config loader.
func NewBridgeServer(backend BridgeBackend, loadConfig func() (*config.Config, error)) *BridgeServer {
	if loadConfig == nil {
		loadConfig = config.LoadOrInit
	}
	sockPath, _ := config.SocketPath()
	return &BridgeServer{
		backend:              backend,
		loadConfig:           loadConfig,
		defaultWait:          bridgeDefaultWaitDuration,
		recoveryPollInterval: bridgeDefaultRecoveryInterval,
		recoveryIdleTimeout:  bridgeDefaultRecoveryIdleTimeout,
		recoveryMaxWait:      bridgeDefaultRecoveryMaxWait,
		recoverySettleDelay:  bridgeDefaultRecoverySettleDelay,
		daemonSocketPath:     sockPath,
		jobs:                 make(map[string]*bridgeJob),
	}
}

type bridgeRPCRequest struct {
	JSONRPC string          `json:"jsonrpc"`
	ID      json.RawMessage `json:"id,omitempty"`
	Method  string          `json:"method"`
	Params  json.RawMessage `json:"params,omitempty"`
}

type bridgeRPCError struct {
	Code    int    `json:"code"`
	Message string `json:"message"`
}

type bridgeRPCResponse struct {
	JSONRPC string          `json:"jsonrpc"`
	ID      json.RawMessage `json:"id,omitempty"`
	Result  any             `json:"result,omitempty"`
	Error   *bridgeRPCError `json:"error,omitempty"`
}

type bridgeRPCNotification struct {
	JSONRPC string `json:"jsonrpc"`
	Method  string `json:"method"`
	Params  any    `json:"params,omitempty"`
}

type bridgeToolDef struct {
	Name        string         `json:"name"`
	Description string         `json:"description"`
	InputSchema map[string]any `json:"inputSchema"`
}

type bridgeToolCallMeta struct {
	ProgressToken json.RawMessage `json:"progressToken,omitempty"`
}

type bridgeToolCallParams struct {
	Name      string              `json:"name"`
	Arguments json.RawMessage     `json:"arguments,omitempty"`
	Meta      *bridgeToolCallMeta `json:"_meta,omitempty"`
}

type bridgeToolContent struct {
	Type string `json:"type"`
	Text string `json:"text"`
}

type bridgeToolCallResult struct {
	Content []bridgeToolContent `json:"content"`
	IsError bool                `json:"isError,omitempty"`
}

// ServeStdio reads newline-delimited JSON-RPC 2.0 requests from r and writes responses to w.
func (s *BridgeServer) ServeStdio(ctx context.Context, r io.Reader, w io.Writer) error {
	sc := bufio.NewScanner(r)
	sc.Buffer(make([]byte, 64*1024), 8<<20)

	var wg sync.WaitGroup
	var writeErrMu sync.Mutex
	var firstWriteErr error
	recordWriteErr := func(err error) {
		if err == nil {
			return
		}
		writeErrMu.Lock()
		if firstWriteErr == nil {
			firstWriteErr = err
		}
		writeErrMu.Unlock()
	}

	// 서버 컨텍스트가 종료되면 남아 있는 백그라운드 작업을 모두 정리한다.
	stopWatch := make(chan struct{})
	go func() {
		select {
		case <-ctx.Done():
			s.cancelAllJobs()
		case <-stopWatch:
		}
	}()
	defer close(stopWatch)

	emitNotify := func(method string, params any) {
		recordWriteErr(s.writeNotification(w, method, params))
	}

	for sc.Scan() {
		if ctx.Err() != nil {
			wg.Wait()
			return ctx.Err()
		}
		line := strings.TrimSpace(sc.Text())
		if line == "" {
			continue
		}

		var req bridgeRPCRequest
		if err := json.Unmarshal([]byte(line), &req); err != nil {
			if werr := s.writeResponse(w, bridgeRPCResponse{
				JSONRPC: "2.0",
				Error:   &bridgeRPCError{Code: -32700, Message: fmt.Sprintf("JSON 파싱 오류: %v", err)},
			}); werr != nil {
				wg.Wait()
				return werr
			}
			continue
		}

		// tools/call은 동시 처리해 장기 대기 중에도 ping·wait·cancel·status 호출이 막히지 않게 한다.
		if req.Method == "tools/call" {
			wg.Add(1)
			go func(req bridgeRPCRequest) {
				defer wg.Done()
				resp, send := s.handleRequest(ctx, req, emitNotify)
				if !send {
					return
				}
				recordWriteErr(s.writeResponse(w, resp))
			}(req)
			continue
		}

		resp, send := s.handleRequest(ctx, req, emitNotify)
		if !send {
			continue
		}
		if err := s.writeResponse(w, resp); err != nil {
			wg.Wait()
			return err
		}
	}
	wg.Wait()
	if err := sc.Err(); err != nil {
		return err
	}
	writeErrMu.Lock()
	defer writeErrMu.Unlock()
	return firstWriteErr
}

func (s *BridgeServer) writeResponse(w io.Writer, resp bridgeRPCResponse) error {
	b, err := json.Marshal(resp)
	if err != nil {
		return err
	}
	s.writeMu.Lock()
	defer s.writeMu.Unlock()
	_, err = w.Write(append(b, '\n'))
	return err
}

func (s *BridgeServer) writeNotification(w io.Writer, method string, params any) error {
	b, err := json.Marshal(bridgeRPCNotification{
		JSONRPC: "2.0",
		Method:  method,
		Params:  params,
	})
	if err != nil {
		return err
	}
	s.writeMu.Lock()
	defer s.writeMu.Unlock()
	_, err = w.Write(append(b, '\n'))
	return err
}

func parseMCPLogLevel(level string) int32 {
	switch strings.ToLower(strings.TrimSpace(level)) {
	case "debug":
		return 1
	case "info":
		return 2
	case "notice":
		return 3
	case "warning":
		return 4
	case "error", "critical", "alert", "emergency":
		return 5
	default:
		return 2
	}
}

func (s *BridgeServer) infoLoggingEnabled() bool {
	lvl := s.logLevel.Load()
	return lvl > 0 && lvl <= 2
}

func (s *BridgeServer) handleRequest(ctx context.Context, req bridgeRPCRequest, notify func(method string, params any)) (bridgeRPCResponse, bool) {
	isNotification := len(req.ID) == 0 || string(req.ID) == "null"

	switch req.Method {
	case "initialize":
		protoVer := bridgeDefaultProtocolVersion
		var p struct {
			ProtocolVersion string `json:"protocolVersion"`
		}
		if len(req.Params) > 0 && json.Unmarshal(req.Params, &p) == nil && strings.TrimSpace(p.ProtocolVersion) != "" {
			protoVer = strings.TrimSpace(p.ProtocolVersion)
		}
		return bridgeRPCResponse{
			JSONRPC: "2.0",
			ID:      req.ID,
			Result: map[string]any{
				"protocolVersion": protoVer,
				"capabilities": map[string]any{
					"tools":   map[string]any{},
					"logging": map[string]any{},
				},
				"serverInfo": map[string]any{
					"name":    bridgeServerName,
					"version": bridgeServerVersion,
				},
			},
		}, true

	case "notifications/initialized", "notifications/cancelled":
		return bridgeRPCResponse{}, false

	case "logging/setLevel":
		var p struct {
			Level string `json:"level"`
		}
		if len(req.Params) > 0 {
			_ = json.Unmarshal(req.Params, &p)
		}
		s.logLevel.Store(parseMCPLogLevel(p.Level))
		if isNotification {
			return bridgeRPCResponse{}, false
		}
		return bridgeRPCResponse{
			JSONRPC: "2.0",
			ID:      req.ID,
			Result:  map[string]any{},
		}, true

	case "ping":
		if isNotification {
			return bridgeRPCResponse{}, false
		}
		return bridgeRPCResponse{
			JSONRPC: "2.0",
			ID:      req.ID,
			Result:  map[string]any{},
		}, true

	case "tools/list":
		return bridgeRPCResponse{
			JSONRPC: "2.0",
			ID:      req.ID,
			Result: map[string]any{
				"tools": s.toolDefinitions(),
			},
		}, true

	case "tools/call":
		var params bridgeToolCallParams
		if err := json.Unmarshal(req.Params, &params); err != nil {
			return bridgeRPCResponse{
				JSONRPC: "2.0",
				ID:      req.ID,
				Error:   &bridgeRPCError{Code: -32602, Message: fmt.Sprintf("tools/call 인자 파싱 오류: %v", err)},
			}, true
		}
		res := s.callTool(ctx, params, notify)
		return bridgeRPCResponse{
			JSONRPC: "2.0",
			ID:      req.ID,
			Result:  res,
		}, true

	default:
		if isNotification {
			return bridgeRPCResponse{}, false
		}
		return bridgeRPCResponse{
			JSONRPC: "2.0",
			ID:      req.ID,
			Error:   &bridgeRPCError{Code: -32601, Message: fmt.Sprintf("지원하지 않는 메서드: %s", req.Method)},
		}, true
	}
}

func (s *BridgeServer) toolDefinitions() []bridgeToolDef {
	return []bridgeToolDef{
		{
			Name:        "gemini_remote_run",
			Description: "Delegate a task or prompt to the Gemini remote-PC agent/MCP quota. Waits up to wait_seconds (default 60s, max 150s) and automatically returns status='running' with progress (status_text, live_tool, thoughts_tail, recent_tools) and a job_id before MCP client timeouts; continue awaiting with gemini_remote_wait.",
			InputSchema: map[string]any{
				"type": "object",
				"properties": map[string]any{
					"prompt": map[string]any{
						"type":        "string",
						"description": "The prompt or task to execute via gemini-remote.",
					},
					"model": map[string]any{
						"type":        "string",
						"description": "Optional Gemini model ID or display title (from gemini_remote_models). Uses configured default_model when omitted.",
					},
					"thinking_level": map[string]any{
						"type":        "string",
						"enum":        []string{"auto", "low", "medium", "high"},
						"description": "Optional thinking level override (auto, low, medium, high). Uses configured default_thinking_level when omitted.",
					},
					"conversation_id": map[string]any{
						"type":        "string",
						"description": "Optional existing conversation_id to continue a multi-turn session.",
					},
					"device_id": map[string]any{
						"type":        "string",
						"description": "Optional target remote device ID. Defaults to configured default_device_id or the local PC.",
					},
					"agent_task": map[string]any{
						"type":        "boolean",
						"description": "Whether to run in remote-PC agent/MCP mode (FeatureModeJanusRemy). Defaults to configured default_agent_task (true).",
					},
					"temporary": map[string]any{
						"type":        "boolean",
						"description": "Whether to run as a temporary conversation not saved in history.",
					},
					"file_paths": map[string]any{
						"type":        "array",
						"items":       map[string]any{"type": "string"},
						"description": "Optional local file paths to attach to the prompt.",
					},
					"wait_seconds": map[string]any{
						"type":        "integer",
						"minimum":     0,
						"maximum":     150,
						"description": "Max seconds to wait inline before returning a running job_id (default 60, 0 = detach immediately, max 150).",
					},
				},
				"required": []string{"prompt"},
			},
		},
		{
			Name:        "gemini_remote_wait",
			Description: "Wait for a background gemini_remote_run job to finish (up to wait_seconds, default 60s, max 150s). Returns the final response when done, or status='running' with status_text, live_tool, thoughts_tail, and recent_tools so you can relay progress and call wait again.",
			InputSchema: map[string]any{
				"type": "object",
				"properties": map[string]any{
					"job_id": map[string]any{
						"type":        "string",
						"description": "Background job_id returned by gemini_remote_run.",
					},
					"wait_seconds": map[string]any{
						"type":        "integer",
						"minimum":     0,
						"maximum":     150,
						"description": "Max seconds to block waiting for completion (default 60, 0 = status check only, max 150).",
					},
				},
				"required": []string{"job_id"},
			},
		},
		{
			Name:        "gemini_remote_cancel",
			Description: "Cancel an in-progress background gemini_remote_run job.",
			InputSchema: map[string]any{
				"type": "object",
				"properties": map[string]any{
					"job_id": map[string]any{
						"type":        "string",
						"description": "Background job_id to cancel.",
					},
				},
				"required": []string{"job_id"},
			},
		},
		{
			Name:        "gemini_remote_models",
			Description: "List available Gemini models for the logged-in account along with current gemini-remote MCP bridge default settings.",
			InputSchema: map[string]any{
				"type":       "object",
				"properties": map[string]any{},
			},
		},
		{
			Name:        "gemini_remote_status",
			Description: "Inspect current gemini-remote MCP bridge settings, device identity, connected folders, and remote status.",
			InputSchema: map[string]any{
				"type":       "object",
				"properties": map[string]any{},
			},
		},
		{
			Name:        "gemini_remote_devices",
			Description: "List registered remote desktop devices available for gemini_remote_run.",
			InputSchema: map[string]any{
				"type":       "object",
				"properties": map[string]any{},
			},
		},
		{
			Name:        "gemini_remote_usage",
			Description: "Show current Gemini remote/app quota usage including 5-hour session usage, weekly limit, remaining percentage, and reset countdown.",
			InputSchema: map[string]any{
				"type":       "object",
				"properties": map[string]any{},
			},
		},
	}
}

func bridgeTextResult(text string) bridgeToolCallResult {
	return bridgeToolCallResult{
		Content: []bridgeToolContent{{Type: "text", Text: text}},
	}
}

func bridgeErrorResult(err error) bridgeToolCallResult {
	return bridgeToolCallResult{
		Content: []bridgeToolContent{{Type: "text", Text: err.Error()}},
		IsError: true,
	}
}

func (s *BridgeServer) callTool(ctx context.Context, params bridgeToolCallParams, notify func(method string, params any)) bridgeToolCallResult {
	var progressToken json.RawMessage
	if params.Meta != nil && len(params.Meta.ProgressToken) > 0 && string(params.Meta.ProgressToken) != "null" {
		progressToken = params.Meta.ProgressToken
	}
	switch params.Name {
	case "gemini_remote_run":
		return s.handleRun(ctx, params.Arguments, progressToken, notify)
	case "gemini_remote_wait":
		return s.handleWait(ctx, params.Arguments, progressToken, notify)
	case "gemini_remote_cancel":
		return s.handleCancel(params.Arguments)
	case "gemini_remote_models":
		return s.handleModels(ctx)
	case "gemini_remote_status":
		return s.handleStatus()
	case "gemini_remote_devices":
		return s.handleDevices(ctx)
	case "gemini_remote_usage":
		return s.handleUsage(ctx)
	default:
		return bridgeErrorResult(fmt.Errorf("알 수 없는 도구입니다: %s", params.Name))
	}
}

type bridgeRunArgs struct {
	Prompt         string   `json:"prompt"`
	Model          string   `json:"model,omitempty"`
	ThinkingLevel  string   `json:"thinking_level,omitempty"`
	ConversationID string   `json:"conversation_id,omitempty"`
	DeviceID       string   `json:"device_id,omitempty"`
	AgentTask      *bool    `json:"agent_task,omitempty"`
	Temporary      *bool    `json:"temporary,omitempty"`
	FilePaths      []string `json:"file_paths,omitempty"`
	WaitSeconds    *int     `json:"wait_seconds,omitempty"`
}

type bridgeWaitArgs struct {
	JobID       string `json:"job_id"`
	WaitSeconds *int   `json:"wait_seconds,omitempty"`
}

type bridgeCancelArgs struct {
	JobID string `json:"job_id"`
}

func (s *BridgeServer) resolveWaitDuration(waitSeconds *int) time.Duration {
	if waitSeconds == nil {
		if s.defaultWait > 0 {
			return s.defaultWait
		}
		return bridgeDefaultWaitDuration
	}
	sec := *waitSeconds
	if sec <= 0 {
		return 0
	}
	d := time.Duration(sec) * time.Second
	if d > bridgeMaxWaitDuration {
		return bridgeMaxWaitDuration
	}
	return d
}

func (s *BridgeServer) cancelAllJobs() {
	s.jobsMu.Lock()
	defer s.jobsMu.Unlock()
	for _, j := range s.jobs {
		j.mu.Lock()
		if j.status == bridgeJobRunning && j.cancel != nil {
			j.status = bridgeJobCancelled
			j.cancel()
		}
		j.mu.Unlock()
	}
}

func (s *BridgeServer) pruneJobsLocked() {
	completed := 0
	for _, id := range s.jobOrder {
		j := s.jobs[id]
		if j == nil {
			continue
		}
		j.mu.Lock()
		isDone := j.status != bridgeJobRunning
		j.mu.Unlock()
		if isDone {
			completed++
		}
	}
	if completed <= bridgeMaxCompletedJobs {
		return
	}
	toDrop := completed - bridgeMaxCompletedJobs
	kept := make([]string, 0, len(s.jobOrder))
	for _, id := range s.jobOrder {
		j := s.jobs[id]
		if j == nil {
			continue
		}
		if toDrop > 0 {
			j.mu.Lock()
			isDone := j.status != bridgeJobRunning
			j.mu.Unlock()
			if isDone {
				delete(s.jobs, id)
				toDrop--
				continue
			}
		}
		kept = append(kept, id)
	}
	s.jobOrder = kept
}

func (s *BridgeServer) startJob(queryParams proto.ChatQueryParams, agentTask bool, deviceName string) *bridgeJob {
	jobCtx, cancel := context.WithCancel(context.Background())
	id := fmt.Sprintf("job_%d", s.jobSeq.Add(1))
	now := time.Now()
	job := &bridgeJob{
		id:             id,
		startedAt:      now,
		lastActivityAt: now,
		status:         bridgeJobRunning,
		phase:          "streaming",
		agentTask:      agentTask,
		conversationID: queryParams.ConversationID,
		modelAttr:      queryParams.ModeVariantID,
		targetDevice:   deviceName,
		seenTools:      make(map[string]int),
		done:           make(chan struct{}),
		cancel:         cancel,
	}

	s.jobsMu.Lock()
	if s.jobs == nil {
		s.jobs = make(map[string]*bridgeJob)
	}
	s.jobs[id] = job
	s.jobOrder = append(s.jobOrder, id)
	s.jobsMu.Unlock()

	go func() {
		defer cancel()
		defer close(job.done)

		var (
			liveHints <-chan struct{}
			turnDone  atomic.Bool
			sseAlive  atomic.Bool
		)
		if agentTask && s.subscribeHints == nil && strings.TrimSpace(s.daemonSocketPath) != "" {
			if evCh, unsub := s.subscribeDaemonEvents(jobCtx); evCh != nil {
				defer unsub()
				hints := make(chan struct{}, 1)
				liveHints = hints
				sseAlive.Store(true)
				go func() {
					defer func() {
						sseAlive.Store(false)
						close(hints)
					}()
					for ev := range evCh {
						job.mu.Lock()
						wantConv := job.conversationID
						matchConv := wantConv != "" && ev.SessionID == wantConv
						isHint := ev.ToolName == "conversations_updated" && (ev.SessionID == "" || matchConv)
						isTurnDone := matchConv && (ev.Phase == "turn_completed" || ev.Phase == "session_ended")
						if matchConv {
							job.recordDaemonEventLocked(ev)
						} else if isHint {
							job.lastActivityAt = time.Now()
						}
						job.mu.Unlock()

						if isTurnDone {
							turnDone.Store(true)
						}
						if isHint || isTurnDone {
							select {
							case hints <- struct{}{}:
							default:
							}
						}
					}
				}()
			}
		}

		final, err := s.backend.StreamQuery(jobCtx, queryParams, func(ch proto.ChatStreamChunk) error {
			job.mu.Lock()
			defer job.mu.Unlock()
			now := time.Now()
			if ch.Delta != "" {
				job.streamedText.WriteString(ch.Delta)
				job.lastActivityAt = now
			} else if ch.Text != "" {
				job.streamedText.Reset()
				job.streamedText.WriteString(ch.Text)
				job.lastActivityAt = now
			}
			if ch.ThoughtsDelta != "" {
				job.thoughts.WriteString(ch.ThoughtsDelta)
				job.lastActivityAt = now
				if tail := tailRunes(job.thoughts.String(), 120); tail != "" {
					job.emitProgressLocked("사고 중: " + tail)
				}
			} else if ch.Thoughts != "" {
				job.thoughts.Reset()
				job.thoughts.WriteString(ch.Thoughts)
				job.lastActivityAt = now
				if tail := tailRunes(ch.Thoughts, 120); tail != "" {
					job.emitProgressLocked("사고 중: " + tail)
				}
			}
			if st := strings.TrimSpace(ch.StatusText); st != "" && st != job.statusText {
				job.statusText = st
				job.lastActivityAt = now
				job.emitProgressLocked(st)
			}
			if ch.ConversationID != "" {
				job.conversationID = ch.ConversationID
			}
			if ch.ModelAttribution != "" {
				job.modelAttr = ch.ModelAttribution
			}
			if ch.TargetDeviceName != "" {
				job.targetDevice = ch.TargetDeviceName
			}
			job.appendActivitiesLocked(ch.ToolActivities)
			return nil
		})

		// StreamQuery 가 3분 안팎에 빈 응답이나 조기 EOF 로 닫혀도 서버 백그라운드 에이전트는 계속 실행될 수 있으므로
		// agent_task 이고 conversation_id 가 확보된 경우 ListConversationTurns 로 최종 응답을 자동 복구한다.
		job.mu.Lock()
		if final != nil {
			if final.ConversationID != "" {
				job.conversationID = final.ConversationID
			}
			if final.ModelAttribution != "" {
				job.modelAttr = final.ModelAttribution
			}
			if final.TargetDeviceName != "" {
				job.targetDevice = final.TargetDeviceName
			}
			if st := strings.TrimSpace(final.StatusText); st != "" {
				job.statusText = st
			}
			if final.Thoughts != "" && job.thoughts.Len() == 0 {
				job.thoughts.WriteString(final.Thoughts)
			}
			job.appendActivitiesLocked(final.ToolActivities)
		}
		body := ""
		if final != nil && final.Text != "" {
			body = final.Text
		}
		if body == "" {
			body = job.streamedText.String()
		}
		convID := job.conversationID
		isCancelled := job.status == bridgeJobCancelled
		job.mu.Unlock()

		shouldRecover := agentTask && convID != "" && !isCancelled && !errors.Is(err, context.Canceled) &&
			((err == nil && strings.TrimSpace(body) == "") || (err != nil && isStreamEOFError(err)))
		if shouldRecover {
			job.mu.Lock()
			job.phase = "recovering"
			job.lastActivityAt = time.Now()
			job.mu.Unlock()

			recBody, recErr := s.recoverAgentTurn(jobCtx, job, convID, queryParams.ParentRequestID, liveHints, &turnDone, &sseAlive)
			if recErr == nil && strings.TrimSpace(recBody) != "" {
				body = recBody
				err = nil
			} else if recErr != nil {
				err = recErr
			}
		}

		job.mu.Lock()
		job.finishedAt = time.Now()
		job.liveTool = ""
		if err != nil {
			if job.status == bridgeJobCancelled || errors.Is(err, context.Canceled) {
				job.status = bridgeJobCancelled
				job.errText = "작업이 취소되었습니다"
				job.finalResult = bridgeErrorResult(fmt.Errorf("작업이 취소되었습니다 (job_id=%s)", job.id))
			} else {
				job.status = bridgeJobFailed
				job.errText = err.Error()
				job.finalResult = bridgeErrorResult(err)
			}
		} else {
			job.status = bridgeJobCompleted
			job.finalResult = formatRunResult(body, job.conversationID, job.modelAttr, job.targetDevice, job.agentTask, job.activities)
		}
		job.mu.Unlock()

		s.jobsMu.Lock()
		s.pruneJobsLocked()
		s.jobsMu.Unlock()
	}()

	return job
}

func isStreamEOFError(err error) bool {
	if err == nil {
		return false
	}
	if errors.Is(err, io.EOF) || errors.Is(err, io.ErrUnexpectedEOF) {
		return true
	}
	low := strings.ToLower(err.Error())
	return strings.Contains(low, "eof") || strings.Contains(low, "stream terminated") || strings.Contains(low, "internal_error")
}

func (s *BridgeServer) getDaemonHTTPClient(sock string) *http.Client {
	s.daemonHTTPMu.Lock()
	defer s.daemonHTTPMu.Unlock()
	if s.daemonHTTP != nil && s.daemonHTTPPath == sock {
		return s.daemonHTTP
	}
	s.daemonHTTPPath = sock
	s.daemonHTTP = newDaemonUnixHTTPClient(sock)
	return s.daemonHTTP
}

func (s *BridgeServer) subscribeDaemonEvents(ctx context.Context) (<-chan daemonToolEvent, func()) {
	sock := strings.TrimSpace(s.daemonSocketPath)
	if sock == "" {
		return nil, func() {}
	}
	subCtx, cancel := context.WithCancel(ctx)
	client := s.getDaemonHTTPClient(sock)
	req, err := http.NewRequestWithContext(subCtx, http.MethodGet, "http://unix/v1/remote/events", nil)
	if err != nil {
		cancel()
		return nil, func() {}
	}
	resp, err := client.Do(req)
	if err != nil {
		cancel()
		return nil, func() {}
	}
	if resp.StatusCode != http.StatusOK {
		_ = resp.Body.Close()
		cancel()
		return nil, func() {}
	}

	events := make(chan daemonToolEvent, 32)
	go func() {
		defer resp.Body.Close()
		defer close(events)
		sc := bufio.NewScanner(resp.Body)
		sc.Buffer(make([]byte, 16*1024), 1<<20)
		curEvent := ""
		for sc.Scan() {
			line := sc.Text()
			if line == "" {
				curEvent = ""
				continue
			}
			if strings.HasPrefix(line, "event:") {
				curEvent = strings.TrimSpace(strings.TrimPrefix(line, "event:"))
				continue
			}
			if curEvent != "tool" || !strings.HasPrefix(line, "data:") {
				continue
			}
			raw := strings.TrimSpace(strings.TrimPrefix(line, "data:"))
			var ev daemonToolEvent
			if json.Unmarshal([]byte(raw), &ev) != nil {
				continue
			}
			select {
			case events <- ev:
			case <-subCtx.Done():
				return
			}
		}
	}()

	return events, cancel
}

func (s *BridgeServer) recoverAgentTurn(
	ctx context.Context,
	job *bridgeJob,
	convID, parentReqID string,
	liveHints <-chan struct{},
	turnDone *atomic.Bool,
	sseAlive *atomic.Bool,
) (string, error) {
	pollInterval := s.recoveryPollInterval
	if pollInterval <= 0 {
		pollInterval = bridgeDefaultRecoveryInterval
	}
	idleTimeout := s.recoveryIdleTimeout
	if idleTimeout <= 0 {
		idleTimeout = bridgeDefaultRecoveryIdleTimeout
	}
	maxWait := s.recoveryMaxWait
	if maxWait <= 0 {
		maxWait = bridgeDefaultRecoveryMaxWait
	}
	settleDelay := s.recoverySettleDelay
	if settleDelay < 0 {
		settleDelay = 0
	}

	now := time.Now()
	absDeadline := now.Add(maxWait)
	lastActivity := now

	hints := liveHints
	if hints == nil && s.subscribeHints != nil {
		h, unsub := s.subscribeHints(ctx, convID)
		defer unsub()
		hints = h
	}

	var (
		lastText        string
		textStableSince time.Time
		lastStatus      string
		lastThoughts    string
		lastToolCount   int
	)

	for {
		now = time.Now()
		turns, err := s.backend.ListConversationTurns(ctx, convID, 1, "")
		if err == nil && turns != nil && len(turns.Turns) > 0 {
			turn := turns.Turns[0]
			if parentReqID == "" || turn.RequestID != parentReqID {
				progressed := false
				job.mu.Lock()
				if turn.ModelAttribution != "" {
					job.modelAttr = turn.ModelAttribution
				}
				if st := strings.TrimSpace(turn.StatusText); st != "" && st != lastStatus {
					lastStatus = st
					job.statusText = st
					job.lastActivityAt = now
					job.emitProgressLocked(st)
					progressed = true
				}
				if turn.Thoughts != "" && turn.Thoughts != lastThoughts {
					lastThoughts = turn.Thoughts
					job.thoughts.Reset()
					job.thoughts.WriteString(turn.Thoughts)
					job.lastActivityAt = now
					if tail := tailRunes(turn.Thoughts, 120); tail != "" {
						job.emitProgressLocked("사고 중: " + tail)
					}
					progressed = true
				}
				if job.appendActivitiesLocked(turn.ToolActivities) {
					progressed = true
				}
				if len(turn.ToolActivities) > lastToolCount {
					lastToolCount = len(turn.ToolActivities)
					progressed = true
				}
				if turn.ModelText != "" {
					job.streamedText.Reset()
					job.streamedText.WriteString(turn.ModelText)
				}
				if job.lastActivityAt.After(lastActivity) {
					lastActivity = job.lastActivityAt
				}
				job.mu.Unlock()

				if progressed {
					lastActivity = now
				}

				if trimmed := strings.TrimSpace(turn.ModelText); trimmed != "" {
					switch turn.CompletenessState {
					case proto.CompletenessComplete:
						return turn.ModelText, nil
					case proto.CompletenessPartial:
						// 서버가 명시적으로 부분 생성(PARTIAL) 상태이므로 완료될 때까지 계속 대기한다.
						if turn.ModelText != lastText {
							lastText = turn.ModelText
							lastActivity = now
						}
						textStableSince = time.Time{}
					default:
						// CompletenessState가 생략된 저장 턴은 첫 감지 직후 잘린 스냅샷일 수 있으므로
						// 텍스트가 더 이상 변하지 않고 settleDelay 동안 안정화된 뒤 확정 반환한다.
						if turn.ModelText != lastText {
							lastText = turn.ModelText
							textStableSince = now
							lastActivity = now
							if settleDelay == 0 {
								return turn.ModelText, nil
							}
						} else {
							need := settleDelay
							if turnDone != nil && turnDone.Load() && need > 2*time.Second {
								need = 2 * time.Second
							}
							if need <= 0 || now.Sub(textStableSince) >= need {
								return turn.ModelText, nil
							}
						}
					}
				}
				if turn.Aborted {
					return "", fmt.Errorf("원격 작업이 중단되었습니다 (conversation_id=%s)", convID)
				}
			}
		} else {
			job.mu.Lock()
			if job.lastActivityAt.After(lastActivity) {
				lastActivity = job.lastActivityAt
			}
			job.mu.Unlock()
		}

		if now.After(absDeadline) || now.Sub(lastActivity) >= idleTimeout {
			if strings.TrimSpace(lastText) != "" {
				return lastText, nil
			}
			return "", fmt.Errorf("원격 작업 응답 대기 시간이 초과되었습니다 (conversation_id=%s, 유휴 %v) — 원격 데몬에서 작업이 계속 진행 중일 수 있습니다", convID, now.Sub(lastActivity).Round(time.Second))
		}

		waitStep := pollInterval
		if strings.TrimSpace(lastText) != "" {
			// 응답 생성 중이거나 안정화 확인 중에는 빠르게 재조회해 완료 직후 지연을 줄인다.
			fastStep := 2 * time.Second
			if settleDelay > 0 && settleDelay/2 < fastStep {
				fastStep = settleDelay / 2
			}
			if fastStep < 10*time.Millisecond {
				fastStep = 10 * time.Millisecond
			}
			if fastStep < waitStep {
				waitStep = fastStep
			}
		} else if hints != nil && (sseAlive == nil || sseAlive.Load() || s.subscribeHints != nil) && waitStep < bridgeSSEFallbackInterval {
			waitStep = bridgeSSEFallbackInterval
		}

		timer := time.NewTimer(waitStep)
		select {
		case <-ctx.Done():
			timer.Stop()
			return "", ctx.Err()
		case _, ok := <-hints:
			timer.Stop()
			if !ok {
				// SSE 연결이 끊어지면 기존 폴링 주기로 폴백한다.
				hints = nil
			} else {
				lastActivity = time.Now()
			}
		case <-timer.C:
		}
	}
}

func (s *BridgeServer) waitForJob(
	ctx context.Context,
	job *bridgeJob,
	wait time.Duration,
	progressToken json.RawMessage,
	notify func(method string, params any),
) bridgeToolCallResult {
	select {
	case <-job.done:
		job.mu.Lock()
		res := job.finalResult
		job.mu.Unlock()
		return res
	default:
	}

	if wait <= 0 {
		return job.snapshotRunning(time.Now())
	}

	var (
		progCh   <-chan progressUpdate
		lastSent time.Time
	)
	wantProgress := notify != nil && (len(progressToken) > 0 || s.infoLoggingEnabled())
	if wantProgress {
		subID, ch := job.subscribeProgress()
		defer job.unsubscribeProgress(subID)
		progCh = ch
	}

	sendUpdate := func(upd progressUpdate) {
		if notify == nil || strings.TrimSpace(upd.Message) == "" {
			return
		}
		now := time.Now()
		if !lastSent.IsZero() && now.Sub(lastSent) < bridgeProgressThrottle {
			return
		}
		lastSent = now
		if len(progressToken) > 0 {
			notify("notifications/progress", map[string]any{
				"progressToken": progressToken,
				"progress":      upd.Progress,
				"message":       upd.Message,
			})
		}
		if s.infoLoggingEnabled() {
			notify("notifications/message", map[string]any{
				"level":  "info",
				"logger": bridgeServerName,
				"data":   upd.Message,
			})
		}
	}

	timer := time.NewTimer(wait)
	defer timer.Stop()

	for {
		select {
		case <-job.done:
			job.mu.Lock()
			res := job.finalResult
			job.mu.Unlock()
			return res
		case upd := <-progCh:
			sendUpdate(upd)
		case <-timer.C:
			return job.snapshotRunning(time.Now())
		case <-ctx.Done():
			return job.snapshotRunning(time.Now())
		}
	}
}

func formatRunResult(body, convID, modelAttr, targetDev string, agentTask bool, activities []proto.RemoteToolActivity) bridgeToolCallResult {
	var out strings.Builder
	out.WriteString(strings.TrimSpace(body))
	if out.Len() == 0 {
		out.WriteString("(빈 응답)")
	}

	var meta []string
	if convID != "" {
		meta = append(meta, "conversation_id="+convID)
	}
	if modelAttr != "" {
		meta = append(meta, "model="+modelAttr)
	}
	if targetDev != "" && agentTask {
		meta = append(meta, "device="+targetDev)
	}
	if len(activities) > 0 {
		names := make([]string, 0, len(activities))
		for _, a := range activities {
			if formatted := formatActivity(a); formatted != "" {
				names = append(names, formatted)
			}
		}
		if len(names) > 0 {
			meta = append(meta, "tools="+strings.Join(names, ", "))
		}
	}
	if len(meta) > 0 {
		out.WriteString("\n\n[")
		out.WriteString(strings.Join(meta, " | "))
		out.WriteString("]")
	}
	return bridgeTextResult(out.String())
}

func (s *BridgeServer) handleWait(ctx context.Context, rawArgs json.RawMessage, progressToken json.RawMessage, notify func(method string, params any)) bridgeToolCallResult {
	var args bridgeWaitArgs
	if len(rawArgs) > 0 {
		if err := json.Unmarshal(rawArgs, &args); err != nil {
			return bridgeErrorResult(fmt.Errorf("인자 파싱 실패: %w", err))
		}
	}
	jobID := strings.TrimSpace(args.JobID)
	if jobID == "" {
		return bridgeErrorResult(fmt.Errorf("job_id 인자가 비어 있습니다"))
	}

	s.jobsMu.Lock()
	job := s.jobs[jobID]
	s.jobsMu.Unlock()
	if job == nil {
		return bridgeErrorResult(fmt.Errorf("작업을 찾을 수 없습니다: %s", jobID))
	}
	return s.waitForJob(ctx, job, s.resolveWaitDuration(args.WaitSeconds), progressToken, notify)
}

func (s *BridgeServer) handleCancel(rawArgs json.RawMessage) bridgeToolCallResult {
	var args bridgeCancelArgs
	if len(rawArgs) > 0 {
		if err := json.Unmarshal(rawArgs, &args); err != nil {
			return bridgeErrorResult(fmt.Errorf("인자 파싱 실패: %w", err))
		}
	}
	jobID := strings.TrimSpace(args.JobID)
	if jobID == "" {
		return bridgeErrorResult(fmt.Errorf("job_id 인자가 비어 있습니다"))
	}

	s.jobsMu.Lock()
	job := s.jobs[jobID]
	s.jobsMu.Unlock()
	if job == nil {
		return bridgeErrorResult(fmt.Errorf("작업을 찾을 수 없습니다: %s", jobID))
	}

	job.mu.Lock()
	prev := job.status
	if job.status == bridgeJobRunning && job.cancel != nil {
		job.status = bridgeJobCancelled
		job.cancel()
	}
	cur := job.status
	job.mu.Unlock()

	b, _ := json.MarshalIndent(map[string]any{
		"job_id":          jobID,
		"previous_status": string(prev),
		"status":          string(cur),
	}, "", "  ")
	return bridgeTextResult(string(b))
}

func (s *BridgeServer) handleRun(ctx context.Context, rawArgs json.RawMessage, progressToken json.RawMessage, notify func(method string, params any)) bridgeToolCallResult {
	var args bridgeRunArgs
	if len(rawArgs) > 0 {
		if err := json.Unmarshal(rawArgs, &args); err != nil {
			return bridgeErrorResult(fmt.Errorf("인자 파싱 실패: %w", err))
		}
	}
	if strings.TrimSpace(args.Prompt) == "" {
		return bridgeErrorResult(fmt.Errorf("prompt 인자가 비어 있습니다"))
	}

	cfg, err := s.loadConfig()
	if err != nil {
		return bridgeErrorResult(fmt.Errorf("설정 로드 실패: %w", err))
	}
	bridge := cfg.GetMCPBridge()

	agentTask := bridge.EffectiveAgentTask()
	if args.AgentTask != nil {
		agentTask = *args.AgentTask
	}
	temporary := bridge.DefaultTemporary
	if args.Temporary != nil {
		temporary = *args.Temporary
	}

	deviceID := strings.TrimSpace(args.DeviceID)
	if deviceID == "" {
		deviceID = strings.TrimSpace(bridge.DefaultDeviceID)
	}
	if deviceID == "" && agentTask {
		deviceID = strings.TrimSpace(cfg.DeviceID)
	}
	deviceName := ""
	if strings.EqualFold(deviceID, cfg.DeviceID) {
		deviceName = cfg.DeviceName
	}

	modelQuery := strings.TrimSpace(args.Model)
	if modelQuery == "" {
		modelQuery = strings.TrimSpace(bridge.DefaultModel)
	}
	if strings.EqualFold(modelQuery, "auto") {
		modelQuery = ""
	}

	thinkingRaw := strings.TrimSpace(args.ThinkingLevel)
	if thinkingRaw == "" {
		thinkingRaw = strings.TrimSpace(bridge.DefaultThinkingLevel)
	}
	_, thinkingVal, err := config.NormalizeThinkingLevel(thinkingRaw)
	if err != nil {
		return bridgeErrorResult(err)
	}

	queryParams := proto.ChatQueryParams{
		Text:           args.Prompt,
		ConversationID: strings.TrimSpace(args.ConversationID),
		Temporary:      temporary,
		AgentTask:      agentTask,
		FilePaths:      args.FilePaths,
	}
	if agentTask {
		queryParams.TargetDeviceID = deviceID
		queryParams.TargetDeviceName = deviceName
	}

	supportsThinking := true
	if modelQuery != "" {
		status, merr := s.backend.GetUserStatus(ctx)
		if merr != nil {
			return bridgeErrorResult(fmt.Errorf("모델 목록 조회 실패: %w", merr))
		}
		if matched, ok := ResolveModel(status.Models, modelQuery); ok {
			queryParams.ModeCategory = matched.ModeCategory
			queryParams.ModeVariantID = matched.ID
			supportsThinking = matched.SupportsThinking
		} else {
			return bridgeErrorResult(fmt.Errorf("모델을 찾을 수 없습니다: %q (gemini_remote_models로 사용 가능한 모델을 확인하세요)", modelQuery))
		}
	}
	if supportsThinking && thinkingVal != 0 {
		queryParams.ThinkingLevel = thinkingVal
	}

	if queryParams.ConversationID != "" {
		if turns, terr := s.backend.ListConversationTurns(ctx, queryParams.ConversationID, 1, ""); terr == nil && turns != nil && len(turns.Turns) > 0 {
			queryParams.ParentRequestID = turns.Turns[0].RequestID
			queryParams.ParentResponseID = turns.Turns[0].ResponseID
		}
	}

	job := s.startJob(queryParams, agentTask, deviceName)
	return s.waitForJob(ctx, job, s.resolveWaitDuration(args.WaitSeconds), progressToken, notify)
}

func (s *BridgeServer) handleModels(ctx context.Context) bridgeToolCallResult {
	cfg, _ := s.loadConfig()
	var bridge config.MCPBridgeConfig
	if cfg != nil {
		bridge = cfg.GetMCPBridge()
	}

	status, err := s.backend.GetUserStatus(ctx)
	if err != nil {
		return bridgeErrorResult(err)
	}

	payload := map[string]any{
		"tier":                   status.Tier,
		"default_model":          bridge.DefaultModel,
		"default_thinking_level": bridge.DefaultThinkingLevel,
		"models":                 status.Models,
	}
	b, _ := json.MarshalIndent(payload, "", "  ")
	return bridgeTextResult(string(b))
}

func (s *BridgeServer) handleStatus() bridgeToolCallResult {
	cfg, err := s.loadConfig()
	if err != nil {
		return bridgeErrorResult(err)
	}
	bridge := cfg.GetMCPBridge()
	payload := map[string]any{
		"device_id":         cfg.DeviceID,
		"device_name":       cfg.DeviceName,
		"remote_enabled":    cfg.IsRemoteEnabled(),
		"connected_folders": cfg.CloneConnectedFolders(),
		"mcp_bridge": map[string]any{
			"default_model":          bridge.DefaultModel,
			"default_thinking_level": bridge.DefaultThinkingLevel,
			"default_device_id":      bridge.DefaultDeviceID,
			"default_agent_task":     bridge.EffectiveAgentTask(),
			"default_temporary":      bridge.DefaultTemporary,
		},
	}
	b, _ := json.MarshalIndent(payload, "", "  ")
	return bridgeTextResult(string(b))
}

func (s *BridgeServer) handleDevices(ctx context.Context) bridgeToolCallResult {
	devs, err := s.backend.ListDevices(ctx)
	if err != nil {
		return bridgeErrorResult(err)
	}
	b, _ := json.MarshalIndent(map[string]any{"devices": devs}, "", "  ")
	return bridgeTextResult(string(b))
}

// UsageBucketSummary는 단일 사용량 버킷(5시간 세션/주간 한도)의 계산된 요약 정보다.
type UsageBucketSummary struct {
	BucketType       int32   `json:"bucket_type"`
	Window           string  `json:"window"`
	Label            string  `json:"label"`
	UsedPercent      float64 `json:"used_percent"`
	RemainingPercent float64 `json:"remaining_percent"`
	RemainingVal     int64   `json:"remaining_val,omitempty"`
	MaxVal           int64   `json:"max_val,omitempty"`
	ResetAtUnix      int64   `json:"reset_at_unix,omitempty"`
	ResetAt          string  `json:"reset_at,omitempty"`
	ResetIn          string  `json:"reset_in,omitempty"`
}

// UsageSummary는 계정 쿼터 사용량 전체 요약이다.
type UsageSummary struct {
	Buckets    []UsageBucketSummary `json:"buckets"`
	NoticeText string               `json:"notice_text,omitempty"`
	SupportURL string               `json:"support_url,omitempty"`
}

func formatResetCountdown(d time.Duration) string {
	if d <= 0 {
		return "곧 초기화"
	}
	totalMins := int64(math.Ceil(d.Minutes()))
	days := totalMins / (60 * 24)
	hours := (totalMins % (60 * 24)) / 60
	mins := totalMins % 60
	switch {
	case days > 0:
		return fmt.Sprintf("%d일 %d시간 후", days, hours)
	case hours > 0:
		return fmt.Sprintf("%d시간 %d분 후", hours, mins)
	default:
		return fmt.Sprintf("%d분 후", mins)
	}
}

// SummarizeUsage는 proto.UsageInfo를 사용률·잔여율·초기화 남은 시간 포함 요약으로 변환한다.
func SummarizeUsage(info *proto.UsageInfo, now time.Time) UsageSummary {
	sum := UsageSummary{
		Buckets: []UsageBucketSummary{},
	}
	if info == nil {
		return sum
	}
	sum.NoticeText = info.NoticeText
	sum.SupportURL = info.SupportURL

	for _, b := range info.Buckets {
		ratio := float64(b.UsedRatio)
		if math.IsNaN(ratio) || math.IsInf(ratio, 0) || ratio < 0 {
			ratio = 0
		}
		if ratio > 1 {
			ratio = 1
		}
		usedPct := math.Round(ratio*1000) / 10
		remPct := math.Round((100.0-usedPct)*10) / 10
		if remPct < 0 {
			remPct = 0
		}

		win := "other"
		switch b.BucketType {
		case 1:
			win = "session_5h"
		case 2:
			win = "weekly"
		}

		item := UsageBucketSummary{
			BucketType:       b.BucketType,
			Window:           win,
			Label:            b.Label,
			UsedPercent:      usedPct,
			RemainingPercent: remPct,
			RemainingVal:     b.RemainingVal,
			MaxVal:           b.MaxVal,
			ResetAtUnix:      b.ResetAtUnix,
		}
		if b.ResetAtUnix > 0 {
			rt := time.Unix(b.ResetAtUnix, 0).In(now.Location())
			item.ResetAt = rt.Format("2006-01-02 15:04 MST")
			item.ResetIn = formatResetCountdown(rt.Sub(now))
		}
		sum.Buckets = append(sum.Buckets, item)
	}
	return sum
}

// FormatUsageText는 UsageSummary를 터미널/챗에서 읽기 좋은 텍스트 표로 포맷한다.
func FormatUsageText(sum UsageSummary) string {
	if len(sum.Buckets) == 0 {
		return "표시할 사용량 한도 정보가 없습니다."
	}
	var out strings.Builder
	out.WriteString("Gemini 원격/앱 쿼터 사용량\n")
	for _, b := range sum.Buckets {
		filled := int(math.Round(b.UsedPercent / 10.0))
		if filled < 0 {
			filled = 0
		}
		if filled > 10 {
			filled = 10
		}
		bar := strings.Repeat("#", filled) + strings.Repeat("-", 10-filled)

		kindTag := b.Label
		switch b.BucketType {
		case 1:
			kindTag = fmt.Sprintf("5시간 세션 (%s)", b.Label)
		case 2:
			kindTag = fmt.Sprintf("주간 한도 (%s)", b.Label)
		}

		out.WriteString(fmt.Sprintf("  - %s: [%s] 사용 %.1f%% / 남음 %.1f%%", kindTag, bar, b.UsedPercent, b.RemainingPercent))
		if b.MaxVal > 0 {
			out.WriteString(fmt.Sprintf(" (%d/%d 남음)", b.RemainingVal, b.MaxVal))
		}
		if b.ResetAt != "" {
			out.WriteString(fmt.Sprintf(" · 초기화: %s (%s)", b.ResetAt, b.ResetIn))
		}
		out.WriteByte('\n')
	}
	if strings.TrimSpace(sum.NoticeText) != "" {
		out.WriteString("  안내: " + strings.TrimSpace(sum.NoticeText) + "\n")
	}
	return strings.TrimRight(out.String(), "\n")
}

func (s *BridgeServer) handleUsage(ctx context.Context) bridgeToolCallResult {
	info, err := s.backend.GetUsageInfo(ctx)
	if err != nil {
		return bridgeErrorResult(fmt.Errorf("사용량 한도 조회 실패: %w", err))
	}
	sum := SummarizeUsage(info, time.Now())
	b, _ := json.MarshalIndent(map[string]any{
		"summary_text": FormatUsageText(sum),
		"usage":        sum,
	}, "", "  ")
	return bridgeTextResult(string(b))
}
