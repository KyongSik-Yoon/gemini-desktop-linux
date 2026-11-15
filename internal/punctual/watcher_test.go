package punctual

import (
	"context"
	"encoding/base64"
	"fmt"
	"io"
	"strings"
	"sync/atomic"
	"testing"
	"time"

	"github.com/sam/gemini-remote-linux/internal/proto"
)

func TestParseWebChannelChunk(t *testing.T) {
	dispatch := proto.NewBuilder()
	dispatch.AppendString(1, "session_key_777")
	dispatch.AppendString(2, "routing_tok_888")

	wakeup := proto.NewBuilder()
	wakeup.AppendMessage(1, dispatch.Bytes())

	objPath := proto.NewBuilder()
	objPath.AppendString(4, "remote-tool-wakeup")

	objChange := proto.NewBuilder()
	objChange.AppendBytes(2, wakeup.Bytes())
	objChange.AppendMessage(6, objPath.Bytes())

	watchResp := proto.NewBuilder()
	watchResp.AppendMessage(2, objChange.Bytes())

	multiResp := proto.NewBuilder()
	multiResp.AppendInt64Always(1, 2)
	multiResp.AppendMessage(2, watchResp.Bytes())

	batch := proto.NewBuilder()
	batch.AppendMessage(1, multiResp.Bytes())

	b64 := base64.StdEncoding.EncodeToString(batch.Bytes())
	chunk := fmt.Sprintf(`[[0, ["c", "SID_12345", null, 8]], [1, ["%s"]]]`, b64)

	if sid := extractSID(chunk); sid != "SID_12345" {
		t.Fatalf("SID 추출 실패: %s", sid)
	}

	aid, payloads, shouldStop := ParseWebChannelChunk(chunk)
	if aid != 1 || len(payloads) != 1 || shouldStop {
		t.Fatalf("청크 파싱 결과 불일치: aid=%d, payloads=%d, stop=%v", aid, len(payloads), shouldStop)
	}

	changes, needsReconnect, err := proto.DecodeMultiWatchResponseBatch(payloads[0])
	if err != nil || len(changes) != 1 || needsReconnect {
		t.Fatalf("MultiWatchResponseBatch 디코딩 실패: %v, len=%d, reconnect=%v", err, len(changes), needsReconnect)
	}
	ev, err := proto.DecodeWakeUpPayload(changes[0].StreamID, changes[0].ObjectPath, changes[0].Payload)
	if err != nil {
		t.Fatalf("WakeUpPayload 디코딩 실패: %v", err)
	}
	if ev.SessionRoutingKey != "session_key_777" || ev.RoutingToken != "routing_tok_888" {
		t.Fatalf("디코딩된 이벤트 값 불일치: %+v", ev)
	}
}

func makeCheckpointChunk(aid int) string {
	watchResp := proto.NewBuilder()
	watchResp.AppendBytes(3, []byte{0x08, 0x01})
	multiResp := proto.NewBuilder()
	multiResp.AppendInt64Always(1, 1)
	multiResp.AppendMessage(2, watchResp.Bytes())
	batch := proto.NewBuilder()
	batch.AppendMessage(1, multiResp.Bytes())
	b64 := base64.StdEncoding.EncodeToString(batch.Bytes())
	return fmt.Sprintf("[[%d, [\"%s\"]]]\n", aid, b64)
}

func TestConsumeBackChannelZombiePunctualTimeout(t *testing.T) {
	pr, pw := io.Pipe()
	defer pw.Close()

	w := &Watcher{
		deviceID:        "DEV-1",
		readTimeout:     500 * time.Millisecond,
		punctualTimeout: 80 * time.Millisecond,
	}

	stopWriter := make(chan struct{})
	defer close(stopWriter)
	go func() {
		ticker := time.NewTicker(15 * time.Millisecond)
		defer ticker.Stop()
		aid := 1
		for {
			select {
			case <-stopWriter:
				return
			case <-ticker.C:
				chunk := fmt.Sprintf("[[%d, [\"noop\"]]]\n", aid)
				aid++
				if _, err := io.WriteString(pw, chunk); err != nil {
					return
				}
			}
		}
	}()

	var lastPunctual atomic.Int64
	lastPunctual.Store(time.Now().UnixNano())

	doneCh := make(chan error, 1)
	go func() {
		_, err := w.consumeBackChannel(context.Background(), pr, 0, &lastPunctual, func(context.Context, *proto.WakeUpEvent) {})
		doneCh <- err
	}()

	select {
	case err := <-doneCh:
		if err == nil || !strings.Contains(err.Error(), "좀비 구독") {
			t.Fatalf("Punctual 좀비 구독 타임아웃 오류 기대, 실제=%v", err)
		}
	case <-time.After(450 * time.Millisecond):
		t.Fatal("noop 수신 중에도 Punctual 워치독이 제때 작동하지 않았습니다")
	}
}

func TestConsumeBackChannelPunctualCheckpointKeepsAlive(t *testing.T) {
	pr, pw := io.Pipe()

	w := &Watcher{
		deviceID:        "DEV-1",
		readTimeout:     500 * time.Millisecond,
		punctualTimeout: 70 * time.Millisecond,
	}

	var lastPunctual atomic.Int64
	lastPunctual.Store(time.Now().UnixNano())

	doneCh := make(chan struct {
		aid int
		err error
	}, 1)
	go func() {
		aid, err := w.consumeBackChannel(context.Background(), pr, 0, &lastPunctual, func(context.Context, *proto.WakeUpEvent) {})
		doneCh <- struct {
			aid int
			err error
		}{aid: aid, err: err}
	}()

	// punctualTimeout(70ms)의 두 배 이상인 140ms 동안 25ms마다 Checkpoint 전송 시 끊기지 않아야 함
	for i := 1; i <= 6; i++ {
		if _, err := io.WriteString(pw, makeCheckpointChunk(i)); err != nil {
			t.Fatalf("Checkpoint 전송 실패(워치독이 조기 종료됨, step=%d): %v", i, err)
		}
		time.Sleep(25 * time.Millisecond)
	}
	_ = pw.Close()

	select {
	case res := <-doneCh:
		if res.err != io.EOF {
			t.Fatalf("정상 종료 시 io.EOF 기대, 실제=%v", res.err)
		}
		if res.aid != 6 {
			t.Fatalf("마지막 AID 불일치: got=%d, want=6", res.aid)
		}
	case <-time.After(500 * time.Millisecond):
		t.Fatal("consumeBackChannel이 파이프 종료 후 제때 반환되지 않았습니다")
	}
}
