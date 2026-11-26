package mcp

import (
	"bytes"
	"context"
	"encoding/json"
	"os"
	"path/filepath"
	"strings"
	"testing"
	"time"

	"github.com/sam/gemini-remote-linux/internal/config"
	"github.com/sam/gemini-remote-linux/internal/proto"
)

type fakeBridgeBackend struct {
	status       *proto.UserStatusResult
	usage        *proto.UsageInfo
	devices      []proto.RegisteredDeviceSummary
	turns        *proto.ListConversationTurnsResult
	turnsFn      func(ctx context.Context, conversationID string) (*proto.ListConversationTurnsResult, error)
	lastParams   proto.ChatQueryParams
	streamChunks []proto.ChatStreamChunk
	streamFinal  *proto.ChatStreamChunk
	streamFn     func(ctx context.Context, params proto.ChatQueryParams, onChunk func(proto.ChatStreamChunk) error) (*proto.ChatStreamChunk, error)
}

func (f *fakeBridgeBackend) GetUserStatus(_ context.Context) (*proto.UserStatusResult, error) {
	return f.status, nil
}

func (f *fakeBridgeBackend) GetUsageInfo(_ context.Context) (*proto.UsageInfo, error) {
	return f.usage, nil
}

func (f *fakeBridgeBackend) ListDevices(_ context.Context) ([]proto.RegisteredDeviceSummary, error) {
	return f.devices, nil
}

func (f *fakeBridgeBackend) ListConversationTurns(ctx context.Context, convID string, _ int32, _ string) (*proto.ListConversationTurnsResult, error) {
	if f.turnsFn != nil {
		return f.turnsFn(ctx, convID)
	}
	return f.turns, nil
}

func (f *fakeBridgeBackend) StreamQuery(ctx context.Context, params proto.ChatQueryParams, onChunk func(proto.ChatStreamChunk) error) (*proto.ChatStreamChunk, error) {
	f.lastParams = params
	if f.streamFn != nil {
		return f.streamFn(ctx, params, onChunk)
	}
	for _, ch := range f.streamChunks {
		if onChunk != nil {
			if err := onChunk(ch); err != nil {
				return nil, err
			}
		}
	}
	return f.streamFinal, nil
}

func runBridgeRPC(t *testing.T, srv *BridgeServer, req string) bridgeRPCResponse {
	t.Helper()
	var out bytes.Buffer
	if err := srv.ServeStdio(context.Background(), strings.NewReader(req+"\n"), &out); err != nil {
		t.Fatalf("ServeStdio 실패: %v", err)
	}
	var resp bridgeRPCResponse
	if err := json.Unmarshal(bytes.TrimSpace(out.Bytes()), &resp); err != nil {
		t.Fatalf("응답 JSON 파싱 실패 (%q): %v", out.String(), err)
	}
	return resp
}

func TestBridgeServerInitializeAndToolsList(t *testing.T) {
	srv := NewBridgeServer(&fakeBridgeBackend{}, func() (*config.Config, error) {
		return &config.Config{DeviceID: "dev-1", DeviceName: "linux-box"}, nil
	})

	initResp := runBridgeRPC(t, srv, `{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-03-26"}}`)
	if initResp.Error != nil {
		t.Fatalf("initialize 오류: %+v", initResp.Error)
	}
	resMap, ok := initResp.Result.(map[string]any)
	if !ok || resMap["protocolVersion"] != "2025-03-26" {
		t.Fatalf("protocolVersion 불일치: %#v", initResp.Result)
	}

	listResp := runBridgeRPC(t, srv, `{"jsonrpc":"2.0","id":2,"method":"tools/list"}`)
	if listResp.Error != nil {
		t.Fatalf("tools/list 오류: %+v", listResp.Error)
	}
	listMap := listResp.Result.(map[string]any)
	tools := listMap["tools"].([]any)
	if len(tools) != 7 {
		t.Fatalf("예상 도구 개수 7, got=%d", len(tools))
	}
}

func TestBridgeServerRunUsesDefaultsAndResolvesModel(t *testing.T) {
	fb := &fakeBridgeBackend{
		status: &proto.UserStatusResult{
			Tier: "ULTRA",
			Models: []proto.BardModeOption{
				{ID: "gemini-3-pro", TitleForPicker: "Pro", ModeCategory: 5, SupportsThinking: true},
				{ID: "gemini-3-flash", TitleForPicker: "Fast", ModeCategory: 6, SupportsThinking: false},
			},
		},
		turns: &proto.ListConversationTurnsResult{
			Turns: []proto.ConversationTurn{
				{RequestID: "r_prev", ResponseID: "rc_prev"},
			},
		},
		streamChunks: []proto.ChatStreamChunk{
			{
				Type:  "data",
				Delta: "실행 완료",
				ToolActivities: []proto.RemoteToolActivity{
					{ID: "tc1", Name: "run_command", Summary: "git status"},
				},
			},
		},
		streamFinal: &proto.ChatStreamChunk{
			Type:             "done",
			Text:             "실행 완료",
			ConversationID:   "c_123",
			ModelAttribution: "gemini-3-pro",
			TargetDeviceName: "linux-box",
		},
	}

	cfg := &config.Config{
		DeviceID:   "dev-local",
		DeviceName: "linux-box",
		MCPBridge: config.MCPBridgeConfig{
			DefaultModel:         "Pro",
			DefaultThinkingLevel: "high",
		},
	}
	srv := NewBridgeServer(fb, func() (*config.Config, error) { return cfg, nil })

	resp := runBridgeRPC(t, srv, `{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"gemini_remote_run","arguments":{"prompt":"git status 확인","conversation_id":"c_123"}}}`)
	if resp.Error != nil {
		t.Fatalf("tools/call 오류: %+v", resp.Error)
	}

	if !fb.lastParams.AgentTask {
		t.Fatal("기본 agent_task가 true여야 합니다")
	}
	if fb.lastParams.TargetDeviceID != "dev-local" {
		t.Fatalf("TargetDeviceID 불일치: got=%q", fb.lastParams.TargetDeviceID)
	}
	if fb.lastParams.ModeCategory != 5 || fb.lastParams.ModeVariantID != "gemini-3-pro" {
		t.Fatalf("모델 해석 불일치: cat=%d variant=%q", fb.lastParams.ModeCategory, fb.lastParams.ModeVariantID)
	}
	if fb.lastParams.ThinkingLevel != proto.ThinkingLevelHigh {
		t.Fatalf("ThinkingLevel 불일치: got=%d want=%d", fb.lastParams.ThinkingLevel, proto.ThinkingLevelHigh)
	}
	if fb.lastParams.ParentRequestID != "r_prev" || fb.lastParams.ParentResponseID != "rc_prev" {
		t.Fatalf("멀티턴 이어쓰기 ID 불일치: req=%q resp=%q", fb.lastParams.ParentRequestID, fb.lastParams.ParentResponseID)
	}

	resMap := resp.Result.(map[string]any)
	content := resMap["content"].([]any)[0].(map[string]any)["text"].(string)
	if !strings.Contains(content, "실행 완료") || !strings.Contains(content, "conversation_id=c_123") || !strings.Contains(content, "run_command(git status)") {
		t.Fatalf("응답 본문/메타데이터 누락: %q", content)
	}
}

func TestBridgeServerRunSkipsThinkingForNonThinkingModel(t *testing.T) {
	fb := &fakeBridgeBackend{
		status: &proto.UserStatusResult{
			Models: []proto.BardModeOption{
				{ID: "gemini-3-flash", TitleForPicker: "Fast", ModeCategory: 6, SupportsThinking: false},
			},
		},
		streamFinal: &proto.ChatStreamChunk{Type: "done", Text: "ok"},
	}
	cfg := &config.Config{DeviceID: "dev-1", DeviceName: "box"}
	srv := NewBridgeServer(fb, func() (*config.Config, error) { return cfg, nil })

	resp := runBridgeRPC(t, srv, `{"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"gemini_remote_run","arguments":{"prompt":"hi","model":"Fast","thinking_level":"high"}}}`)
	if resp.Error != nil {
		t.Fatalf("tools/call 오류: %+v", resp.Error)
	}
	if fb.lastParams.ThinkingLevel != 0 {
		t.Fatalf("SupportsThinking=false 모델은 ThinkingLevel=0이어야 합니다: got=%d", fb.lastParams.ThinkingLevel)
	}
}

func TestBridgeServerUsage(t *testing.T) {
	fb := &fakeBridgeBackend{
		usage: &proto.UsageInfo{
			Buckets: []proto.UsageBucket{
				{BucketType: 1, Label: "현재 사용량", UsedRatio: 0.35, ResetAtUnix: 1795665000, RemainingVal: 65, MaxVal: 100},
				{BucketType: 2, Label: "주간 한도", UsedRatio: 0.125, ResetAtUnix: 1796097000},
			},
		},
	}
	srv := NewBridgeServer(fb, func() (*config.Config, error) { return &config.Config{}, nil })

	resp := runBridgeRPC(t, srv, `{"jsonrpc":"2.0","id":5,"method":"tools/call","params":{"name":"gemini_remote_usage","arguments":{}}}`)
	if resp.Error != nil {
		t.Fatalf("gemini_remote_usage 오류: %+v", resp.Error)
	}
	resMap := resp.Result.(map[string]any)
	text := resMap["content"].([]any)[0].(map[string]any)["text"].(string)
	if !strings.Contains(text, "session_5h") || !strings.Contains(text, "weekly") || !strings.Contains(text, "사용 35.0% / 남음 65.0%") || !strings.Contains(text, "사용 12.5% / 남음 87.5%") {
		t.Fatalf("usage 출력 불일치: %s", text)
	}
}

func TestBridgeServerAsyncJobDetachesAndCompletesOnWait(t *testing.T) {
	started := make(chan struct{})
	release := make(chan struct{})
	fb := &fakeBridgeBackend{
		streamFn: func(ctx context.Context, _ proto.ChatQueryParams, onChunk func(proto.ChatStreamChunk) error) (*proto.ChatStreamChunk, error) {
			if onChunk != nil {
				_ = onChunk(proto.ChatStreamChunk{
					Type:           "data",
					Delta:          "중간 진행 중...",
					ConversationID: "c_async",
					ToolActivities: []proto.RemoteToolActivity{{ID: "t1", Name: "read_file", Summary: "README.md"}},
				})
			}
			close(started)
			select {
			case <-release:
				return &proto.ChatStreamChunk{
					Type:             "done",
					Text:             "장기 작업 최종 결과",
					ConversationID:   "c_async",
					ModelAttribution: "4 Neon",
				}, nil
			case <-ctx.Done():
				return nil, ctx.Err()
			}
		},
	}

	srv := NewBridgeServer(fb, func() (*config.Config, error) { return &config.Config{DeviceID: "dev-1"}, nil })
	srv.defaultWait = 20 * time.Millisecond

	runResp := runBridgeRPC(t, srv, `{"jsonrpc":"2.0","id":10,"method":"tools/call","params":{"name":"gemini_remote_run","arguments":{"prompt":"오래 걸리는 작업"}}}`)
	if runResp.Error != nil {
		t.Fatalf("run 오류: %+v", runResp.Error)
	}
	<-started

	runText := runResp.Result.(map[string]any)["content"].([]any)[0].(map[string]any)["text"].(string)
	var runningPayload map[string]any
	if err := json.Unmarshal([]byte(runText), &runningPayload); err != nil {
		t.Fatalf("running JSON 파싱 실패 (%q): %v", runText, err)
	}
	if runningPayload["status"] != "running" {
		t.Fatalf("status=running 기대, got=%v", runningPayload["status"])
	}
	jobID, _ := runningPayload["job_id"].(string)
	if jobID == "" {
		t.Fatalf("job_id 누락: %v", runningPayload)
	}
	if runningPayload["conversation_id"] != "c_async" {
		t.Fatalf("중간 conversation_id 누락: %v", runningPayload)
	}

	// 작업을 완료시키고 gemini_remote_wait로 최종 결과를 회수한다.
	close(release)
	waitReq := `{"jsonrpc":"2.0","id":11,"method":"tools/call","params":{"name":"gemini_remote_wait","arguments":{"job_id":"` + jobID + `","wait_seconds":5}}}`
	waitResp := runBridgeRPC(t, srv, waitReq)
	if waitResp.Error != nil {
		t.Fatalf("wait 오류: %+v", waitResp.Error)
	}
	waitText := waitResp.Result.(map[string]any)["content"].([]any)[0].(map[string]any)["text"].(string)
	if !strings.Contains(waitText, "장기 작업 최종 결과") || !strings.Contains(waitText, "conversation_id=c_async") || !strings.Contains(waitText, "read_file(README.md)") {
		t.Fatalf("최종 결과 불일치: %q", waitText)
	}
}

func TestBridgeServerCancelRunningJob(t *testing.T) {
	started := make(chan struct{})
	cancelled := make(chan struct{})
	fb := &fakeBridgeBackend{
		streamFn: func(ctx context.Context, _ proto.ChatQueryParams, _ func(proto.ChatStreamChunk) error) (*proto.ChatStreamChunk, error) {
			close(started)
			<-ctx.Done()
			close(cancelled)
			return nil, ctx.Err()
		},
	}

	srv := NewBridgeServer(fb, func() (*config.Config, error) { return &config.Config{DeviceID: "dev-1"}, nil })

	runResp := runBridgeRPC(t, srv, `{"jsonrpc":"2.0","id":20,"method":"tools/call","params":{"name":"gemini_remote_run","arguments":{"prompt":"취소 테스트","wait_seconds":0}}}`)
	<-started
	runText := runResp.Result.(map[string]any)["content"].([]any)[0].(map[string]any)["text"].(string)
	var runningPayload map[string]any
	if err := json.Unmarshal([]byte(runText), &runningPayload); err != nil {
		t.Fatalf("running JSON 파싱 실패: %v", err)
	}
	jobID := runningPayload["job_id"].(string)

	cancelResp := runBridgeRPC(t, srv, `{"jsonrpc":"2.0","id":21,"method":"tools/call","params":{"name":"gemini_remote_cancel","arguments":{"job_id":"`+jobID+`"}}}`)
	if cancelResp.Error != nil {
		t.Fatalf("cancel 오류: %+v", cancelResp.Error)
	}
	select {
	case <-cancelled:
	case <-time.After(2 * time.Second):
		t.Fatal("취소 신호가 StreamQuery 컨텍스트로 전파되지 않았습니다")
	}

	waitResp := runBridgeRPC(t, srv, `{"jsonrpc":"2.0","id":22,"method":"tools/call","params":{"name":"gemini_remote_wait","arguments":{"job_id":"`+jobID+`","wait_seconds":2}}}`)
	resMap := waitResp.Result.(map[string]any)
	if resMap["isError"] != true {
		t.Fatalf("취소된 작업 wait 결과는 isError=true여야 합니다: %#v", resMap)
	}
}

func TestBridgeServerRecoversFromEarlyEmptyStreamEOF(t *testing.T) {
	calls := 0
	fb := &fakeBridgeBackend{
		streamFn: func(_ context.Context, _ proto.ChatQueryParams, onChunk func(proto.ChatStreamChunk) error) (*proto.ChatStreamChunk, error) {
			if onChunk != nil {
				_ = onChunk(proto.ChatStreamChunk{
					Type:           "data",
					ConversationID: "c_eof",
					ToolActivities: []proto.RemoteToolActivity{{ID: "t1", Name: "view_file", Summary: "main.go"}},
				})
			}
			// 3분경 StreamQuery 가 빈 응답으로 조기 종료되는 상황을 시뮬레이션한다.
			return &proto.ChatStreamChunk{Type: "done", Text: "", ConversationID: "c_eof"}, nil
		},
		turnsFn: func(_ context.Context, convID string) (*proto.ListConversationTurnsResult, error) {
			calls++
			if calls < 3 {
				return &proto.ListConversationTurnsResult{
					Turns: []proto.ConversationTurn{{ConversationID: convID, RequestID: "r_new", ModelText: ""}},
				}, nil
			}
			return &proto.ListConversationTurnsResult{
				Turns: []proto.ConversationTurn{{
					ConversationID:    convID,
					RequestID:         "r_new",
					ModelText:         "조기 EOF 이후 자동 복구된 최종 응답",
					ModelAttribution:  "4 Neon",
					CompletenessState: proto.CompletenessComplete,
				}},
			}, nil
		},
	}

	srv := NewBridgeServer(fb, func() (*config.Config, error) { return &config.Config{DeviceID: "dev-1"}, nil })
	srv.daemonSocketPath = "" // 데몬 미실행 환경의 폴링 폴백 검증
	srv.recoveryPollInterval = 10 * time.Millisecond
	srv.recoveryMaxWait = 2 * time.Second

	resp := runBridgeRPC(t, srv, `{"jsonrpc":"2.0","id":30,"method":"tools/call","params":{"name":"gemini_remote_run","arguments":{"prompt":"긴 작업","wait_seconds":2}}}`)
	if resp.Error != nil {
		t.Fatalf("run 오류: %+v", resp.Error)
	}
	text := resp.Result.(map[string]any)["content"].([]any)[0].(map[string]any)["text"].(string)
	if !strings.Contains(text, "조기 EOF 이후 자동 복구된 최종 응답") || !strings.Contains(text, "conversation_id=c_eof") {
		t.Fatalf("조기 EOF 복구 실패: %q", text)
	}
	if calls < 3 {
		t.Fatalf("복구 폴링이 충분히 호출되지 않았습니다: calls=%d", calls)
	}
}

func TestBridgeServerRecoverWakesImmediatelyOnPunctualHint(t *testing.T) {
	calls := 0
	hintCh := make(chan struct{}, 1)
	fb := &fakeBridgeBackend{
		streamFn: func(_ context.Context, _ proto.ChatQueryParams, _ func(proto.ChatStreamChunk) error) (*proto.ChatStreamChunk, error) {
			return &proto.ChatStreamChunk{Type: "done", Text: "", ConversationID: "c_hint"}, nil
		},
		turnsFn: func(_ context.Context, convID string) (*proto.ListConversationTurnsResult, error) {
			calls++
			if calls == 1 {
				return &proto.ListConversationTurnsResult{
					Turns: []proto.ConversationTurn{{ConversationID: convID, RequestID: "r_hint", ModelText: ""}},
				}, nil
			}
			return &proto.ListConversationTurnsResult{
				Turns: []proto.ConversationTurn{{
					ConversationID:    convID,
					RequestID:         "r_hint",
					ModelText:         "Punctual 힌트로 즉시 회수된 응답",
					ModelAttribution:  "4 Neon",
					CompletenessState: proto.CompletenessComplete,
				}},
			}, nil
		},
	}

	srv := NewBridgeServer(fb, func() (*config.Config, error) { return &config.Config{DeviceID: "dev-1"}, nil })
	srv.recoveryPollInterval = 30 * time.Second
	srv.recoveryMaxWait = 2 * time.Second
	srv.subscribeHints = func(_ context.Context, _ string) (<-chan struct{}, func()) {
		return hintCh, func() {}
	}

	go func() {
		time.Sleep(25 * time.Millisecond)
		hintCh <- struct{}{}
	}()

	start := time.Now()
	resp := runBridgeRPC(t, srv, `{"jsonrpc":"2.0","id":31,"method":"tools/call","params":{"name":"gemini_remote_run","arguments":{"prompt":"힌트 즉시 회수","wait_seconds":2}}}`)
	elapsed := time.Since(start)
	if resp.Error != nil {
		t.Fatalf("run 오류: %+v", resp.Error)
	}
	text := resp.Result.(map[string]any)["content"].([]any)[0].(map[string]any)["text"].(string)
	if !strings.Contains(text, "Punctual 힌트로 즉시 회수된 응답") {
		t.Fatalf("힌트 복구 응답 불일치: %q", text)
	}
	if elapsed > 1*time.Second {
		t.Fatalf("Punctual 힌트가 즉시 깨우지 못했습니다: elapsed=%v", elapsed)
	}
}

func TestBridgeServerRecoverWaitsForCompleteAndSettle(t *testing.T) {
	calls := 0
	fb := &fakeBridgeBackend{
		streamFn: func(_ context.Context, _ proto.ChatQueryParams, _ func(proto.ChatStreamChunk) error) (*proto.ChatStreamChunk, error) {
			return &proto.ChatStreamChunk{Type: "done", Text: "", ConversationID: "c_partial"}, nil
		},
		turnsFn: func(_ context.Context, convID string) (*proto.ListConversationTurnsResult, error) {
			calls++
			switch calls {
			case 1:
				return &proto.ListConversationTurnsResult{
					Turns: []proto.ConversationTurn{{
						ConversationID:    convID,
						RequestID:         "r_p",
						ModelText:         "앞부분만 생성 중...",
						CompletenessState: proto.CompletenessPartial,
					}},
				}, nil
			case 2:
				return &proto.ListConversationTurnsResult{
					Turns: []proto.ConversationTurn{{
						ConversationID: convID,
						RequestID:      "r_p",
						ModelText:      "앞부분만 생성 중... 중간까지",
					}},
				}, nil
			default:
				return &proto.ListConversationTurnsResult{
					Turns: []proto.ConversationTurn{{
						ConversationID: convID,
						RequestID:      "r_p",
						ModelText:      "앞부분만 생성 중... 중간까지 끝까지 완료!",
					}},
				}, nil
			}
		},
	}

	srv := NewBridgeServer(fb, func() (*config.Config, error) { return &config.Config{DeviceID: "dev-1"}, nil })
	srv.daemonSocketPath = ""
	srv.recoveryPollInterval = 15 * time.Millisecond
	srv.recoverySettleDelay = 30 * time.Millisecond
	srv.recoveryMaxWait = 2 * time.Second

	resp := runBridgeRPC(t, srv, `{"jsonrpc":"2.0","id":32,"method":"tools/call","params":{"name":"gemini_remote_run","arguments":{"prompt":"긴 리뷰","wait_seconds":2}}}`)
	if resp.Error != nil {
		t.Fatalf("run 오류: %+v", resp.Error)
	}
	text := resp.Result.(map[string]any)["content"].([]any)[0].(map[string]any)["text"].(string)
	if !strings.Contains(text, "끝까지 완료!") {
		t.Fatalf("미완성 텍스트가 조기 반환되었습니다: %q (calls=%d)", text, calls)
	}
	if calls < 4 {
		t.Fatalf("안정화 재조회 없이 조기 반환되었습니다: calls=%d", calls)
	}
}

func TestBridgeServerRecoverIdleTimeoutReturnsError(t *testing.T) {
	fb := &fakeBridgeBackend{
		streamFn: func(_ context.Context, _ proto.ChatQueryParams, _ func(proto.ChatStreamChunk) error) (*proto.ChatStreamChunk, error) {
			return &proto.ChatStreamChunk{Type: "done", Text: "", ConversationID: "c_timeout"}, nil
		},
		turnsFn: func(_ context.Context, convID string) (*proto.ListConversationTurnsResult, error) {
			return &proto.ListConversationTurnsResult{
				Turns: []proto.ConversationTurn{{ConversationID: convID, RequestID: "r_t", ModelText: ""}},
			}, nil
		},
	}

	srv := NewBridgeServer(fb, func() (*config.Config, error) { return &config.Config{DeviceID: "dev-1"}, nil })
	srv.daemonSocketPath = ""
	srv.recoveryPollInterval = 10 * time.Millisecond
	srv.recoveryIdleTimeout = 45 * time.Millisecond
	srv.recoveryMaxWait = 2 * time.Second

	resp := runBridgeRPC(t, srv, `{"jsonrpc":"2.0","id":33,"method":"tools/call","params":{"name":"gemini_remote_run","arguments":{"prompt":"무응답 작업","wait_seconds":2}}}`)
	if resp.Error != nil {
		t.Fatalf("RPC 레벨 오류: %+v", resp.Error)
	}
	resMap := resp.Result.(map[string]any)
	if resMap["isError"] != true {
		t.Fatalf("유휴 타임아웃 시 (빈 응답) 정상 완료가 아니라 isError=true여야 합니다: %#v", resMap)
	}
	text := resMap["content"].([]any)[0].(map[string]any)["text"].(string)
	if !strings.Contains(text, "대기 시간이 초과") {
		t.Fatalf("타임아웃 오류 메시지 불일치: %q", text)
	}
}

func TestBridgeServerProgressPayloadAndNotifications(t *testing.T) {
	release := make(chan struct{})
	fb := &fakeBridgeBackend{
		streamFn: func(ctx context.Context, _ proto.ChatQueryParams, onChunk func(proto.ChatStreamChunk) error) (*proto.ChatStreamChunk, error) {
			time.Sleep(15 * time.Millisecond)
			if onChunk != nil {
				_ = onChunk(proto.ChatStreamChunk{
					Type:           "data",
					ConversationID: "c_prog",
					StatusText:     "변경 사항을 저장하고 커밋하는 중",
					ThoughtsDelta:  "테스트 통과 여부를 먼저 점검해야 한다.",
					ToolActivities: []proto.RemoteToolActivity{
						{ID: "call_1", Name: "Running command", Explanation: "릴리즈 바이너리 빌드 및 로컬 설치 실행"},
					},
				})
			}
			select {
			case <-release:
				return &proto.ChatStreamChunk{Type: "done", Text: "완료", ConversationID: "c_prog"}, nil
			case <-ctx.Done():
				return nil, ctx.Err()
			}
		},
	}

	srv := NewBridgeServer(fb, func() (*config.Config, error) { return &config.Config{DeviceID: "dev-1"}, nil })
	srv.daemonSocketPath = ""
	srv.defaultWait = 80 * time.Millisecond

	reqs := strings.Join([]string{
		`{"jsonrpc":"2.0","id":40,"method":"logging/setLevel","params":{"level":"info"}}`,
		`{"jsonrpc":"2.0","id":41,"method":"tools/call","params":{"name":"gemini_remote_run","arguments":{"prompt":"진행 중계 테스트"},"_meta":{"progressToken":"tok-41"}}}`,
	}, "\n") + "\n"

	var out bytes.Buffer
	if err := srv.ServeStdio(context.Background(), strings.NewReader(reqs), &out); err != nil {
		t.Fatalf("ServeStdio 실패: %v", err)
	}
	close(release)

	lines := strings.Split(strings.TrimSpace(out.String()), "\n")
	var gotProgress, gotLog bool
	var runResp bridgeRPCResponse
	for _, line := range lines {
		var raw map[string]any
		if err := json.Unmarshal([]byte(line), &raw); err != nil {
			t.Fatalf("라인 JSON 파싱 실패 (%q): %v", line, err)
		}
		if method, ok := raw["method"].(string); ok {
			switch method {
			case "notifications/progress":
				params := raw["params"].(map[string]any)
				if params["progressToken"] == "tok-41" && params["message"] != "" {
					gotProgress = true
				}
			case "notifications/message":
				params := raw["params"].(map[string]any)
				if params["level"] == "info" && params["data"] != "" {
					gotLog = true
				}
			}
			continue
		}
		var resp bridgeRPCResponse
		_ = json.Unmarshal([]byte(line), &resp)
		if string(resp.ID) == "41" {
			runResp = resp
		}
	}

	if !gotProgress {
		t.Fatalf("notifications/progress 알림이 전송되지 않았습니다: %s", out.String())
	}
	if !gotLog {
		t.Fatalf("notifications/message 로그 알림이 전송되지 않았습니다: %s", out.String())
	}
	runText := runResp.Result.(map[string]any)["content"].([]any)[0].(map[string]any)["text"].(string)
	var payload map[string]any
	if err := json.Unmarshal([]byte(runText), &payload); err != nil {
		t.Fatalf("running JSON 파싱 실패: %v", err)
	}
	if payload["status_text"] != "변경 사항을 저장하고 커밋하는 중" {
		t.Fatalf("status_text 누락/불일치: %#v", payload)
	}
	if !strings.Contains(payload["thoughts_tail"].(string), "테스트 통과 여부") {
		t.Fatalf("thoughts_tail 누락/불일치: %#v", payload)
	}
	recent := payload["recent_tools"].([]any)
	if len(recent) == 0 || !strings.Contains(recent[0].(string), "릴리즈 바이너리 빌드 및 로컬 설치 실행") {
		t.Fatalf("recent_tools에 Explanation이 포함되지 않았습니다: %#v", recent)
	}
}

func TestBridgeJobEnrichesRunningMCPToolsAndPreservesLastTool(t *testing.T) {
	now := time.Now()
	job := &bridgeJob{
		id:             "job_mcp",
		startedAt:      now.Add(-25 * time.Second),
		lastActivityAt: now.Add(-5 * time.Second),
		status:         bridgeJobRunning,
		phase:          "streaming",
		agentTask:      true,
		conversationID: "c_mcp",
		seenTools:      make(map[string]int),
	}

	job.mu.Lock()
	job.recordDaemonEventLocked(daemonToolEvent{
		SessionID: "c_mcp",
		ToolName:  "execute_mcp",
		ArgsBrief: "git/git_status",
		Phase:     "started",
	})
	job.recordDaemonEventLocked(daemonToolEvent{
		SessionID: "c_mcp",
		ToolName:  "execute_mcp",
		ArgsBrief: "git/git_status",
		Phase:     "finished",
	})
	job.recordDaemonEventLocked(daemonToolEvent{
		SessionID: "c_mcp",
		ToolName:  "execute_mcp",
		ArgsBrief: "git/git_show",
		Phase:     "started",
	})
	job.recordDaemonEventLocked(daemonToolEvent{
		SessionID: "c_mcp",
		ToolName:  "execute_mcp",
		ArgsBrief: "git/git_show",
		Phase:     "finished",
	})
	job.appendActivitiesLocked([]proto.RemoteToolActivity{
		{ID: "a1", Name: "Running MCP tool"},
		{ID: "a2", Name: "Running MCP tool"},
	})
	job.mu.Unlock()

	snap := job.snapshotRunning(now)
	var payload map[string]any
	if err := json.Unmarshal([]byte(snap.Content[0].Text), &payload); err != nil {
		t.Fatalf("snapshot JSON 파싱 실패: %v", err)
	}
	if _, hasLive := payload["live_tool"]; hasLive {
		t.Fatalf("종료 후 live_tool은 비어야 합니다: %#v", payload)
	}
	if payload["last_tool"] != "execute_mcp: git/git_show" {
		t.Fatalf("last_tool 보존 실패: %#v", payload["last_tool"])
	}
	recent := payload["recent_tools"].([]any)
	if len(recent) != 2 || recent[0] != "Running MCP tool(git/git_status)" || recent[1] != "Running MCP tool(git/git_show)" {
		t.Fatalf("Running MCP tool 상세 보강 실패: %#v", recent)
	}
}

func TestBridgeJobProgressEmitSeqMonotonicallyIncreases(t *testing.T) {
	job := &bridgeJob{
		id:        "job_seq",
		status:    bridgeJobRunning,
		seenTools: make(map[string]int),
	}
	subID, ch := job.subscribeProgress()
	defer job.unsubscribeProgress(subID)

	job.mu.Lock()
	job.emitProgressLocked("단계 1: 파일 검색 중")
	job.mu.Unlock()
	u1 := <-ch

	job.mu.Lock()
	job.emitProgressLocked("단계 2: 결과 요약 중")
	job.mu.Unlock()
	u2 := <-ch

	if u1.Progress != 1 || u2.Progress != 2 || u2.Progress <= u1.Progress {
		t.Fatalf("progress 값이 단조 증가해야 합니다: u1=%+v u2=%+v", u1, u2)
	}
}

func TestBridgeServerAdaptiveRunEarlyReturn(t *testing.T) {
	release := make(chan struct{})
	fb := &fakeBridgeBackend{
		streamFn: func(ctx context.Context, _ proto.ChatQueryParams, onChunk func(proto.ChatStreamChunk) error) (*proto.ChatStreamChunk, error) {
			time.Sleep(15 * time.Millisecond)
			if onChunk != nil {
				_ = onChunk(proto.ChatStreamChunk{
					Type:           "data",
					ConversationID: "c_early",
					StatusText:     "원격 저장소 로그 확인 중",
				})
			}
			select {
			case <-release:
				return &proto.ChatStreamChunk{Type: "done", Text: "완료", ConversationID: "c_early"}, nil
			case <-ctx.Done():
				return nil, ctx.Err()
			}
		},
	}

	srv := NewBridgeServer(fb, func() (*config.Config, error) { return &config.Config{DeviceID: "dev-1"}, nil })
	srv.daemonSocketPath = ""
	srv.defaultRunWait = 350 * time.Millisecond
	srv.runEarlyMinWait = 40 * time.Millisecond

	start := time.Now()
	resp := runBridgeRPC(t, srv, `{"jsonrpc":"2.0","id":50,"method":"tools/call","params":{"name":"gemini_remote_run","arguments":{"prompt":"조기 브리핑 테스트"}}}`)
	elapsed := time.Since(start)
	close(release)

	if resp.Error != nil {
		t.Fatalf("run 오류: %+v", resp.Error)
	}
	if elapsed >= 250*time.Millisecond {
		t.Fatalf("초기 브리핑이 도착했는데 조기 반환되지 않았습니다: elapsed=%v", elapsed)
	}
	text := resp.Result.(map[string]any)["content"].([]any)[0].(map[string]any)["text"].(string)
	if !strings.Contains(text, "원격 저장소 로그 확인 중") {
		t.Fatalf("조기 반환 페이로드에 status_text 누락: %s", text)
	}
}

func TestBridgeServerAdaptiveWaitExtendsWhenIdleAndWakesOnChange(t *testing.T) {
	release := make(chan struct{})
	triggerSecond := make(chan struct{})
	fb := &fakeBridgeBackend{
		streamFn: func(ctx context.Context, _ proto.ChatQueryParams, onChunk func(proto.ChatStreamChunk) error) (*proto.ChatStreamChunk, error) {
			if onChunk != nil {
				_ = onChunk(proto.ChatStreamChunk{
					Type:           "data",
					ConversationID: "c_adapt",
					StatusText:     "1단계 분석 중",
				})
			}
			select {
			case <-triggerSecond:
				if onChunk != nil {
					_ = onChunk(proto.ChatStreamChunk{
						Type:           "data",
						ConversationID: "c_adapt",
						StatusText:     "2단계 테스트 실행 중",
					})
				}
			case <-ctx.Done():
				return nil, ctx.Err()
			}
			select {
			case <-release:
				return &proto.ChatStreamChunk{Type: "done", Text: "최종 완료", ConversationID: "c_adapt"}, nil
			case <-ctx.Done():
				return nil, ctx.Err()
			}
		},
	}

	srv := NewBridgeServer(fb, func() (*config.Config, error) { return &config.Config{DeviceID: "dev-1"}, nil })
	srv.daemonSocketPath = ""
	srv.defaultRunWait = 30 * time.Millisecond
	srv.runEarlyMinWait = 0
	srv.defaultWait = 45 * time.Millisecond
	srv.waitIdleMax = 400 * time.Millisecond

	runResp := runBridgeRPC(t, srv, `{"jsonrpc":"2.0","id":60,"method":"tools/call","params":{"name":"gemini_remote_run","arguments":{"prompt":"적응형 wait 테스트"}}}`)
	runText := runResp.Result.(map[string]any)["content"].([]any)[0].(map[string]any)["text"].(string)
	var running map[string]any
	if err := json.Unmarshal([]byte(runText), &running); err != nil {
		t.Fatalf("running JSON 파싱 실패: %v", err)
	}
	jobID := running["job_id"].(string)

	// baseWait(45ms) 시점에는 변화가 없어 연장 대기하고, 100ms 시점에 새 상태가 들어오면 즉시 반환되어야 한다.
	go func() {
		time.Sleep(100 * time.Millisecond)
		close(triggerSecond)
	}()

	start := time.Now()
	waitResp := runBridgeRPC(t, srv, `{"jsonrpc":"2.0","id":61,"method":"tools/call","params":{"name":"gemini_remote_wait","arguments":{"job_id":"`+jobID+`"}}}`)
	elapsed := time.Since(start)
	close(release)

	if waitResp.Error != nil {
		t.Fatalf("wait 오류: %+v", waitResp.Error)
	}
	if elapsed < 80*time.Millisecond || elapsed >= 320*time.Millisecond {
		t.Fatalf("적응형 연장 대기 후 변화 즉시 반환 범위(80ms~320ms) 이탈: elapsed=%v", elapsed)
	}
	waitText := waitResp.Result.(map[string]any)["content"].([]any)[0].(map[string]any)["text"].(string)
	if !strings.Contains(waitText, "2단계 테스트 실행 중") {
		t.Fatalf("새 status_text 누락: %s", waitText)
	}
}

func TestBridgeServerLiveStatusFileLifecycle(t *testing.T) {
	statusDir := t.TempDir()
	chunkSent := make(chan struct{})
	release := make(chan struct{})
	fb := &fakeBridgeBackend{
		streamFn: func(ctx context.Context, _ proto.ChatQueryParams, onChunk func(proto.ChatStreamChunk) error) (*proto.ChatStreamChunk, error) {
			if onChunk != nil {
				_ = onChunk(proto.ChatStreamChunk{
					Type:           "data",
					ConversationID: "c_live",
					StatusText:     "가평 날씨 검색 중\n줄바꿈제거확인",
					ToolActivities: []proto.RemoteToolActivity{{ID: "t1", Name: "Google Search"}},
				})
			}
			close(chunkSent)
			select {
			case <-release:
				return &proto.ChatStreamChunk{Type: "done", Text: "끝", ConversationID: "c_live"}, nil
			case <-ctx.Done():
				return nil, ctx.Err()
			}
		},
	}

	srv := NewBridgeServer(fb, func() (*config.Config, error) { return &config.Config{DeviceID: "dev-1"}, nil })
	srv.daemonSocketPath = ""
	srv.SetLiveStatusDir(statusDir)
	srv.defaultWait = 40 * time.Millisecond

	req := `{"jsonrpc":"2.0","id":70,"method":"tools/call","params":{"name":"gemini_remote_run","arguments":{"prompt":"상태 파일 테스트"},"_meta":{"antigravity.google/conversation_id":"conv-abc-123"}}}`
	runResp := runBridgeRPC(t, srv, req)
	<-chunkSent
	if runResp.Error != nil {
		t.Fatalf("run 오류: %+v", runResp.Error)
	}

	livePath := filepath.Join(statusDir, "mcp-live-status-conv-abc-123.txt")
	// 쓰로틀(1s) 때문에 초기 파일은 startJob에서 이미 생성되어 있어야 한다.
	raw, err := os.ReadFile(livePath)
	if err != nil {
		t.Fatalf("실시간 상태 파일이 생성되지 않았습니다: %v", err)
	}
	lines := strings.Split(strings.TrimRight(string(raw), "\n"), "\n")
	if len(lines) < 7 || lines[2] != "streaming" || lines[6] != "conv-abc-123" || !strings.Contains(lines[3], "가평 날씨 검색 중 줄바꿈제거확인") {
		t.Fatalf("실시간 상태 파일 포맷/내용 불일치: %q", string(raw))
	}

	// 작업 완료 후 상태 파일이 자동 삭제되어야 한다.
	runText := runResp.Result.(map[string]any)["content"].([]any)[0].(map[string]any)["text"].(string)
	var running map[string]any
	_ = json.Unmarshal([]byte(runText), &running)
	jobID := running["job_id"].(string)

	close(release)
	_ = runBridgeRPC(t, srv, `{"jsonrpc":"2.0","id":71,"method":"tools/call","params":{"name":"gemini_remote_wait","arguments":{"job_id":"`+jobID+`","wait_seconds":2}}}`)

	deadline := time.Now().Add(1 * time.Second)
	for {
		if _, err := os.Stat(livePath); os.IsNotExist(err) {
			break
		}
		if time.Now().After(deadline) {
			t.Fatalf("작업 완료 후 실시간 상태 파일이 삭제되지 않았습니다: %s", livePath)
		}
		time.Sleep(15 * time.Millisecond)
	}
}
