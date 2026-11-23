package punctual

import (
	"bufio"
	"bytes"
	"context"
	"encoding/base64"
	"encoding/json"
	"fmt"
	"io"
	"math/rand"
	"net/http"
	"net/url"
	"os"
	"strconv"
	"strings"
	"sync"
	"sync/atomic"
	"time"

	"golang.org/x/net/http2"

	"github.com/sam/gemini-remote-linux/internal/auth"
	"github.com/sam/gemini-remote-linux/internal/proto"
)

const (
	SignalerBaseURL = "https://signaler-pa.googleapis.com"
	PunctualProject = "assistant-bard"

	// Signaler MultiWatch 구독이 서버 측에서 약 20분 만에 묵시 만료되므로 12분마다 선제 회전한다.
	DefaultSessionMaxAge           = 12 * time.Minute
	DefaultReadWatchdogTimeout     = 90 * time.Second
	DefaultPunctualWatchdogTimeout = 3 * time.Minute
	maxWebChannelChunkBytes        = 16 * 1024 * 1024
)

// Handler는 WakeUpEvent가 도착했을 때 호출되는 콜백이다.
type Handler func(ctx context.Context, ev *proto.WakeUpEvent)

// Watcher는 signaler-pa.googleapis.com의 Punctual WebChannel V8 스트림을 구독한다.
type Watcher struct {
	authMgr         *auth.Manager
	deviceID        string
	httpClient      *http.Client
	baseURL         string
	sessionMaxAge   time.Duration
	readTimeout     time.Duration
	punctualTimeout time.Duration
	connected       atomic.Bool
	connMu          sync.RWMutex
	onConnChange    func(bool)
}

// SetOnConnectionChange는 Punctual WebChannel 연결 상태 변화를 통지받을 콜백을 등록한다.
func (w *Watcher) SetOnConnectionChange(fn func(bool)) {
	w.connMu.Lock()
	w.onConnChange = fn
	w.connMu.Unlock()
}

// IsConnected는 현재 Punctual WebChannel 스트림이 연결되어 있는지 반환한다.
func (w *Watcher) IsConnected() bool {
	return w.connected.Load()
}

func (w *Watcher) setConnected(v bool) {
	prev := w.connected.Swap(v)
	if prev == v {
		return
	}
	w.connMu.RLock()
	fn := w.onConnChange
	w.connMu.RUnlock()
	if fn != nil {
		fn(v)
	}
}

func NewWatcher(authMgr *auth.Manager, deviceID string) *Watcher {
	tr := &http.Transport{
		Proxy:                 http.ProxyFromEnvironment,
		ForceAttemptHTTP2:     true,
		MaxIdleConns:          10,
		IdleConnTimeout:       90 * time.Second,
		TLSHandshakeTimeout:   15 * time.Second,
		ExpectContinueTimeout: 1 * time.Second,
	}
	if h2Tr, err := http2.ConfigureTransports(tr); err == nil {
		// 유휴 H2 TCP 연결 단절 감지를 위해 30초 유휴 시 PING 전송
		h2Tr.ReadIdleTimeout = 30 * time.Second
		h2Tr.PingTimeout = 15 * time.Second
	}
	return &Watcher{
		authMgr:  authMgr,
		deviceID: deviceID,
		httpClient: &http.Client{
			Transport: tr,
			Timeout:   0, // 스트리밍 BackChannel을 위해 타임아웃 해제
		},
		baseURL:         SignalerBaseURL,
		sessionMaxAge:   DefaultSessionMaxAge,
		readTimeout:     DefaultReadWatchdogTimeout,
		punctualTimeout: DefaultPunctualWatchdogTimeout,
	}
}

func (w *Watcher) effectiveBaseURL() string {
	if w.baseURL != "" {
		return w.baseURL
	}
	return SignalerBaseURL
}

func (w *Watcher) effectiveSessionMaxAge() time.Duration {
	if w.sessionMaxAge > 0 {
		return w.sessionMaxAge
	}
	return DefaultSessionMaxAge
}

func (w *Watcher) effectiveReadTimeout() time.Duration {
	if w.readTimeout > 0 {
		return w.readTimeout
	}
	return DefaultReadWatchdogTimeout
}

func (w *Watcher) effectivePunctualTimeout() time.Duration {
	if w.punctualTimeout > 0 {
		return w.punctualTimeout
	}
	return DefaultPunctualWatchdogTimeout
}

// Run은 컨텍스트 취소 시까지 지수 백오프로 Punctual WebChannel 연결을 유지한다.
func (w *Watcher) Run(ctx context.Context, handler Handler) error {
	backoff := 2 * time.Second
	for {
		select {
		case <-ctx.Done():
			return ctx.Err()
		default:
		}

		start := time.Now()
		err := w.watchOnce(ctx, handler)
		if ctx.Err() != nil {
			return ctx.Err()
		}
		if time.Since(start) > 15*time.Second {
			backoff = 2 * time.Second
		}
		if err == nil {
			fmt.Printf("[punctual] 세션 선제 갱신 주기(%v) 도래 -> 새 WebChannel 세션 즉시 연결\n", w.effectiveSessionMaxAge())
			continue
		}
		fmt.Printf("[punctual] 스트림 연결 종료 (%v), %v 후 재연결...\n", err, backoff)
		select {
		case <-ctx.Done():
			return ctx.Err()
		case <-time.After(backoff):
		}
		if backoff < 30*time.Second {
			backoff *= 2
		}
	}
}

func (w *Watcher) chooseServer(ctx context.Context, token string) (string, error) {
	reqCtx, cancel := context.WithTimeout(ctx, 15*time.Second)
	defer cancel()

	reqBytes := proto.EncodeChooseServerRequest(PunctualProject, "bard-client-sync")
	req, err := http.NewRequestWithContext(reqCtx, http.MethodPost, w.effectiveBaseURL()+"/punctual/v1/chooseServer", bytes.NewReader(reqBytes))
	if err != nil {
		return "", err
	}
	req.Header.Set("Authorization", "Bearer "+token)
	req.Header.Set("Content-Type", "application/x-protobuf")
	req.Header.Set("Accept", "application/x-protobuf")
	req.Header.Set("X-Goog-Encode-Response-If-Executable", "base64")

	resp, err := w.httpClient.Do(req)
	if err != nil {
		return "", err
	}
	defer resp.Body.Close()
	body, _ := io.ReadAll(resp.Body)
	if resp.StatusCode != http.StatusOK {
		return "", fmt.Errorf("chooseServer HTTP %d: %s", resp.StatusCode, string(body))
	}
	if sid, derr := proto.DecodeChooseServerResponse(body); derr == nil {
		return sid, nil
	}
	if decoded, berr := base64.StdEncoding.DecodeString(strings.TrimSpace(string(body))); berr == nil {
		return proto.DecodeChooseServerResponse(decoded)
	}
	return proto.DecodeChooseServerResponse(body)
}

// ProbeServer는 현재 OAuth 토큰으로 Punctual chooseServer 엔드포인트 응답이 정상인지 1회 점검한다.
func (w *Watcher) ProbeServer(ctx context.Context) error {
	token, err := w.authMgr.AccessToken(ctx)
	if err != nil {
		return fmt.Errorf("액세스 토큰 획득 실패: %w", err)
	}
	sid, err := w.chooseServer(ctx, token)
	if err != nil {
		return err
	}
	if strings.TrimSpace(sid) == "" {
		return fmt.Errorf("chooseServer 응답에 세션 ID가 비어 있습니다")
	}
	return nil
}

func (w *Watcher) watchOnce(parentCtx context.Context, handler Handler) error {
	// Signaler MultiWatch 구독이 약 20분 만에 묵시 만료되므로 만료 도래 전 선제 회전한다.
	ctx, cancel := context.WithTimeout(parentCtx, w.effectiveSessionMaxAge())
	defer cancel()

	token, err := w.authMgr.AccessToken(ctx)
	if err != nil {
		return err
	}

	gsessionID, err := w.chooseServer(ctx, token)
	if err != nil {
		return fmt.Errorf("chooseServer 실패: %w", err)
	}

	// Step 1: Forward Channel POST로 SID 발급 + MultiWatchRequestBatch 전송
	rid := 10000 + rand.Intn(80000)
	zx := randomZX()
	batchBytes := proto.EncodeMultiWatchRequestBatch(PunctualProject)
	b64Batch := base64.StdEncoding.EncodeToString(batchBytes)
	formBody := fmt.Sprintf("count=1&ofs=0&req0___data__=%s", url.QueryEscape(b64Batch))

	fwdURL := fmt.Sprintf("%s/punctual/multi-watch/channel?VER=8&gsessionid=%s&RID=%d&CVER=22&zx=%s&t=1",
		w.effectiveBaseURL(), url.QueryEscape(gsessionID), rid, zx)

	fwdReq, err := http.NewRequestWithContext(ctx, http.MethodPost, fwdURL, strings.NewReader(formBody))
	if err != nil {
		return err
	}
	fwdReq.Header.Set("Authorization", "Bearer "+token)
	fwdReq.Header.Set("Content-Type", "application/x-www-form-urlencoded")
	fwdReq.Header.Set("X-Client-Protocol", "webchannel")
	fwdReq.Header.Set("X-WebChannel-Content-Type", "application/x-protobuf")

	fwdResp, err := w.httpClient.Do(fwdReq)
	if err != nil {
		return fmt.Errorf("ForwardChannel 요청 실패: %w", err)
	}
	fwdBody, _ := io.ReadAll(fwdResp.Body)
	fwdResp.Body.Close()
	if fwdResp.StatusCode != http.StatusOK {
		return fmt.Errorf("ForwardChannel HTTP %d: %s", fwdResp.StatusCode, string(fwdBody))
	}

	sid := extractSID(string(fwdBody))
	if sid == "" {
		return fmt.Errorf("ForwardChannel 응답에서 SID 추출 실패: %s", string(fwdBody))
	}
	fmt.Printf("[punctual] WebChannel 세션 연결됨 (gsessionid=%s, SID=%s)\n", gsessionID, sid)
	w.setConnected(true)
	defer w.setConnected(false)

	// Step 2: BackChannel GET 스트리밍 루프 (BackChannel 재연결 간에도 마지막 Punctual 활성 시각을 유지)
	var lastPunctualNano atomic.Int64
	lastPunctualNano.Store(time.Now().UnixNano())

	aid := 0
	for {
		if ctx.Err() != nil {
			if parentCtx.Err() == nil {
				return nil // 계획된 세션 회전
			}
			return parentCtx.Err()
		}
		token, err = w.authMgr.AccessToken(ctx)
		if err != nil {
			return err
		}

		backURL := fmt.Sprintf("%s/punctual/multi-watch/channel?VER=8&gsessionid=%s&SID=%s&RID=rpc&CI=0&AID=%d&TYPE=xmlhttp&zx=%s&t=1",
			w.effectiveBaseURL(), url.QueryEscape(gsessionID), url.QueryEscape(sid), aid, randomZX())

		backReq, err := http.NewRequestWithContext(ctx, http.MethodGet, backURL, nil)
		if err != nil {
			return err
		}
		backReq.Header.Set("Authorization", "Bearer "+token)
		backReq.Header.Set("X-Client-Protocol", "webchannel")
		backReq.Header.Set("X-WebChannel-Content-Type", "application/x-protobuf")

		backResp, err := w.httpClient.Do(backReq)
		if err != nil {
			if ctx.Err() != nil && parentCtx.Err() == nil {
				return nil
			}
			return fmt.Errorf("BackChannel 연결 오류: %w", err)
		}
		if backResp.StatusCode != http.StatusOK {
			b, _ := io.ReadAll(backResp.Body)
			backResp.Body.Close()
			return fmt.Errorf("BackChannel HTTP %d: %s", backResp.StatusCode, string(b))
		}

		newAID, streamErr := w.consumeBackChannel(parentCtx, backResp.Body, aid, &lastPunctualNano, handler)
		backResp.Body.Close()
		aid = newAID
		if ctx.Err() != nil && parentCtx.Err() == nil {
			return nil
		}
		if streamErr != nil && streamErr != io.EOF {
			return streamErr
		}
	}
}

func (w *Watcher) consumeBackChannel(handlerCtx context.Context, body io.ReadCloser, lastAID int, lastPunctualNano *atomic.Int64, handler Handler) (int, error) {
	reader := bufio.NewReaderSize(body, 64*1024)
	aid := lastAID

	if lastPunctualNano == nil {
		lastPunctualNano = &atomic.Int64{}
	}
	if lastPunctualNano.Load() == 0 {
		lastPunctualNano.Store(time.Now().UnixNano())
	}

	readTimeout := w.effectiveReadTimeout()
	punctualTimeout := w.effectivePunctualTimeout()

	elapsed := time.Since(time.Unix(0, lastPunctualNano.Load()))
	initialPunctualWait := punctualTimeout - elapsed
	if elapsed < 0 {
		initialPunctualWait = punctualTimeout
	} else if initialPunctualWait <= 0 {
		initialPunctualWait = time.Millisecond
	}

	// 전송 계층(["noop"] 포함) 읽기 워치독과 Punctual 활성 프레임(WatchStart/ChangeBatch/Checkpoint) 워치독을 분리해 감시한다.
	doneCh := make(chan struct{})
	defer close(doneCh)
	readActivityCh := make(chan struct{}, 1)
	punctualActivityCh := make(chan struct{}, 1)

	var watchdogMu sync.Mutex
	var watchdogErr error
	setWatchdogErr := func(err error) {
		watchdogMu.Lock()
		if watchdogErr == nil {
			watchdogErr = err
		}
		watchdogMu.Unlock()
	}

	resetTimer := func(t *time.Timer, d time.Duration) {
		if !t.Stop() {
			select {
			case <-t.C:
			default:
			}
		}
		t.Reset(d)
	}

	go func() {
		readTimer := time.NewTimer(readTimeout)
		punctualTimer := time.NewTimer(initialPunctualWait)
		defer readTimer.Stop()
		defer punctualTimer.Stop()
		for {
			select {
			case <-doneCh:
				return
			case <-readActivityCh:
				resetTimer(readTimer, readTimeout)
			case <-punctualActivityCh:
				resetTimer(punctualTimer, punctualTimeout)
			case <-readTimer.C:
				fmt.Printf("[punctual] BackChannel %v 읽기 타임아웃 감지 -> 스트림 강제 재연결\n", readTimeout)
				setWatchdogErr(fmt.Errorf("BackChannel %v 읽기 타임아웃(좀비 연결)", readTimeout))
				_ = body.Close()
				return
			case <-punctualTimer.C:
				fmt.Printf("[punctual] Punctual 구독 %v 무응답(좀비 구독) 감지 -> 새 세션 재구독\n", punctualTimeout)
				setWatchdogErr(fmt.Errorf("Punctual 구독 %v 무응답(좀비 구독)", punctualTimeout))
				_ = body.Close()
				return
			}
		}
	}()

	touchRead := func() {
		select {
		case readActivityCh <- struct{}{}:
		default:
		}
	}
	touchPunctual := func() {
		select {
		case punctualActivityCh <- struct{}{}:
		default:
		}
	}

	for {
		// WebChannel 청크는 "<길이>\n<JSON 페이로드>" 또는 줄 단위 JSON 배열로 도착한다.
		line, err := reader.ReadString('\n')
		if err != nil {
			watchdogMu.Lock()
			wErr := watchdogErr
			watchdogMu.Unlock()
			if wErr != nil {
				return aid, wErr
			}
			return aid, err
		}
		touchRead()
		trimmed := strings.TrimSpace(line)
		if trimmed == "" {
			continue
		}

		var chunk string
		if length, lerr := strconv.Atoi(trimmed); lerr == nil && length > 0 {
			if length > maxWebChannelChunkBytes {
				return aid, fmt.Errorf("WebChannel 청크 크기 초과: %d bytes", length)
			}
			buf := make([]byte, length)
			if _, rerr := io.ReadFull(reader, buf); rerr != nil {
				watchdogMu.Lock()
				wErr := watchdogErr
				watchdogMu.Unlock()
				if wErr != nil {
					return aid, wErr
				}
				return aid, rerr
			}
			touchRead()
			chunk = string(buf)
		} else if strings.HasPrefix(trimmed, "[") {
			chunk = trimmed
		} else {
			continue
		}

		newAID, payloads, shouldStop := ParseWebChannelChunk(chunk)
		if newAID > aid {
			aid = newAID
		}
		reconnectNeeded := shouldStop
		for _, p := range payloads {
			changes, hasWatchActivity, needsReconnect, err := proto.DecodeMultiWatchResponseBatchWithActivity(p)
			if hasWatchActivity {
				lastPunctualNano.Store(time.Now().UnixNano())
				touchPunctual()
			}
			if needsReconnect {
				reconnectNeeded = true
			}
			if err != nil {
				continue
			}
			for _, ch := range changes {
				if os.Getenv("GEMINI_DEBUG_WIRE") == "1" {
					fmt.Printf("[punctual] 이벤트 수신 (stream=%d, path=%s, len=%d, hex=%x)\n", ch.StreamID, ch.ObjectPath, len(ch.Payload), ch.Payload)
				}
				ev, err := proto.DecodeWakeUpPayload(ch.StreamID, ch.ObjectPath, ch.Payload)
				if err != nil || ev == nil {
					if err != nil {
						fmt.Printf("[punctual] 이벤트 참고: %v\n", err)
					}
					continue
				}
				if ev.RefreshDeviceStatus && ev.SessionRoutingKey == "" {
					fmt.Printf("[punctual] 기기 상태 갱신 신호 수신 (remote_enabled=%v)\n", ev.RefreshRemoteEnabled)
				} else if ev.IsClientSyncHint {
					fmt.Printf("[punctual] 대화방 갱신 힌트 감지 (routing_key=%s)\n", ev.SessionRoutingKey)
				} else {
					fmt.Printf("[punctual] 원격 실행 트리거 확인 (routing_key=%s, task=%s)\n", ev.SessionRoutingKey, ev.ScheduledTaskName)
				}
				if ev.TargetDeviceID != "" && !strings.EqualFold(ev.TargetDeviceID, w.deviceID) {
					continue
				}
				go handler(handlerCtx, ev)
			}
		}
		if reconnectNeeded {
			return aid, fmt.Errorf("서버 스트림 종료 또는 Resync 요청으로 세션 재구독")
		}
	}
}

// ParseWebChannelChunk는 WebChannel JSON 배열 청크에서 마지막 AID, 디코딩된 Protobuf 바이트 목록, 종료 제어 신호 여부를 추출한다.
func ParseWebChannelChunk(chunk string) (int, [][]byte, bool) {
	var outer []any
	if err := json.Unmarshal([]byte(chunk), &outer); err != nil {
		return 0, nil, false
	}
	maxAID := 0
	var out [][]byte
	shouldStop := false

	for _, elem := range outer {
		pair, ok := elem.([]any)
		if !ok || len(pair) < 2 {
			continue
		}
		if f, ok := pair[0].(float64); ok && int(f) > maxAID {
			maxAID = int(f)
		}
		inner, ok := pair[1].([]any)
		if !ok || len(inner) == 0 {
			continue
		}
		if tag, ok := inner[0].(string); ok && (tag == "stop" || tag == "close") {
			shouldStop = true
			continue
		}
		for _, item := range inner {
			switch val := item.(type) {
			case string:
				if val == "noop" || val == "c" {
					continue
				}
				if decoded, err := base64.StdEncoding.DecodeString(val); err == nil && len(decoded) > 0 {
					out = append(out, decoded)
				}
			case map[string]any:
				if raw, ok := val["__data__"].(string); ok {
					if decoded, err := base64.StdEncoding.DecodeString(raw); err == nil && len(decoded) > 0 {
						out = append(out, decoded)
					}
				}
			}
		}
	}
	return maxAID, out, shouldStop
}

func extractSID(body string) string {
	var outer []any
	for _, line := range strings.Split(body, "\n") {
		line = strings.TrimSpace(line)
		if !strings.HasPrefix(line, "[") {
			continue
		}
		if err := json.Unmarshal([]byte(line), &outer); err != nil {
			continue
		}
		for _, elem := range outer {
			pair, ok := elem.([]any)
			if !ok || len(pair) < 2 {
				continue
			}
			inner, ok := pair[1].([]any)
			if !ok || len(inner) < 2 {
				continue
			}
			if tag, ok := inner[0].(string); ok && tag == "c" {
				if sid, ok := inner[1].(string); ok {
					return sid
				}
			}
		}
	}
	return ""
}

func randomZX() string {
	const chars = "abcdefghijklmnopqrstuvwxyz0123456789"
	b := make([]byte, 12)
	for i := range b {
		b[i] = chars[rand.Intn(len(chars))]
	}
	return string(b)
}
