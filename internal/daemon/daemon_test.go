package daemon

import (
	"context"
	"sync"
	"sync/atomic"
	"testing"
	"time"

	"github.com/sam/gemini-remote-linux/internal/proto"
	"github.com/sam/gemini-remote-linux/internal/robin"
)

func TestHandleWakeUpPreservesHandshakeSession(t *testing.T) {
	ctx, cancel := context.WithCancel(context.Background())
	defer cancel()

	var runCount atomic.Int32
	hintStarted := make(chan struct{})
	finishHint := make(chan struct{})
	hintDone := make(chan struct{})
	var hintCanceled atomic.Bool

	d := &Daemon{
		activeSessions: make(map[string]*sessionEntry),
		lastHintProbe:  make(map[string]time.Time),
		sessionRunner: func(sCtx context.Context, routingKey, routingToken string, hooks robin.SessionHooks) error {
			n := runCount.Add(1)
			if n == 1 {
				close(hintStarted)
				select {
				case <-sCtx.Done():
					hintCanceled.Store(true)
					return sCtx.Err()
				case <-finishHint:
					if hooks.OnToolCallStart != nil {
						hooks.OnToolCallStart()
					}
					if hooks.OnToolCallEnd != nil {
						hooks.OnToolCallEnd()
					}
					return nil
				}
			}
			return nil
		},
	}

	go func() {
		d.handleWakeUpEvent(ctx, &proto.WakeUpEvent{
			SessionRoutingKey: "c_race1",
			IsClientSyncHint:  true,
		})
		close(hintDone)
	}()

	<-hintStarted

	// 힌트 세션 시작 직후 도착한 WakeUp은 기존 세션을 취소하지 않고 보존해야 한다.
	d.handleWakeUpEvent(ctx, &proto.WakeUpEvent{
		SessionRoutingKey: "c_race1",
		RoutingToken:      "tok_first",
	})

	if hintCanceled.Load() {
		t.Fatal("초기 핸드셰이크 중인 힌트 세션이 후속 WakeUp에 의해 취소되었습니다")
	}
	if got := runCount.Load(); got != 1 {
		t.Fatalf("세션 실행 횟수 불일치: got=%d, want=1", got)
	}

	close(finishHint)
	<-hintDone

	if got := runCount.Load(); got != 1 {
		t.Fatalf("도구 호출을 완료한 세션 뒤에 불필요한 재실행이 발생했습니다: runCount=%d", got)
	}
}

func TestHandleWakeUpResumesPendingAfterEarlyExit(t *testing.T) {
	ctx, cancel := context.WithCancel(context.Background())
	defer cancel()

	var mu sync.Mutex
	var calls []string
	firstStarted := make(chan struct{})
	allowFirstExit := make(chan struct{})
	secondDone := make(chan struct{})

	d := &Daemon{
		activeSessions: make(map[string]*sessionEntry),
		lastHintProbe:  make(map[string]time.Time),
		sessionRunner: func(sCtx context.Context, routingKey, routingToken string, hooks robin.SessionHooks) error {
			mu.Lock()
			calls = append(calls, routingToken)
			idx := len(calls)
			mu.Unlock()
			if idx == 1 {
				close(firstStarted)
				<-allowFirstExit
				// 도구 호출 없이(connected=false 모사) 조기 종료
				return nil
			}
			close(secondDone)
			return nil
		},
	}

	doneFirstGoroutine := make(chan struct{})
	go func() {
		d.handleWakeUpEvent(ctx, &proto.WakeUpEvent{
			SessionRoutingKey: "c_early1",
			IsClientSyncHint:  true,
		})
		close(doneFirstGoroutine)
	}()

	<-firstStarted
	d.handleWakeUpEvent(ctx, &proto.WakeUpEvent{
		SessionRoutingKey: "c_early1",
		RoutingToken:      "tok_pending_99",
	})
	close(allowFirstExit)

	select {
	case <-secondDone:
	case <-time.After(time.Second):
		t.Fatal("조기 종료된 힌트 세션 이후 보류된 WakeUp(tok_pending_99)이 승계 실행되지 않았습니다")
	}
	<-doneFirstGoroutine

	mu.Lock()
	defer mu.Unlock()
	if len(calls) != 2 || calls[0] != "" || calls[1] != "tok_pending_99" {
		t.Fatalf("실행 토큰 이력 불일치: %v", calls)
	}
}

func TestHandleWakeUpReplacesIdleSession(t *testing.T) {
	ctx, cancel := context.WithCancel(context.Background())
	defer cancel()

	enteredIdle := make(chan struct{})
	firstCanceled := make(chan struct{})
	secondDone := make(chan struct{})
	var mu sync.Mutex
	var calls []string

	d := &Daemon{
		activeSessions: make(map[string]*sessionEntry),
		lastHintProbe:  make(map[string]time.Time),
		sessionRunner: func(sCtx context.Context, routingKey, routingToken string, hooks robin.SessionHooks) error {
			mu.Lock()
			calls = append(calls, routingToken)
			idx := len(calls)
			mu.Unlock()
			if idx == 1 {
				if hooks.OnToolCallStart != nil {
					hooks.OnToolCallStart()
				}
				if hooks.OnToolCallEnd != nil {
					hooks.OnToolCallEnd()
				}
				if hooks.OnIdle != nil {
					hooks.OnIdle(true)
				}
				close(enteredIdle)
				<-sCtx.Done()
				close(firstCanceled)
				return sCtx.Err()
			}
			close(secondDone)
			return nil
		},
	}

	go d.handleWakeUpEvent(ctx, &proto.WakeUpEvent{
		SessionRoutingKey: "c_idle1",
		RoutingToken:      "tok_turn1",
	})

	<-enteredIdle

	go d.handleWakeUpEvent(ctx, &proto.WakeUpEvent{
		SessionRoutingKey: "c_idle1",
		RoutingToken:      "tok_turn2",
	})

	select {
	case <-firstCanceled:
	case <-time.After(time.Second):
		t.Fatal("유휴 대기 세션이 새 WakeUp 수신 시 취소되지 않았습니다")
	}
	select {
	case <-secondDone:
	case <-time.After(time.Second):
		t.Fatal("유휴 대기 세션 교체 후 새 WakeUp 세션이 실행되지 않았습니다")
	}

	mu.Lock()
	defer mu.Unlock()
	if len(calls) != 2 || calls[0] != "tok_turn1" || calls[1] != "tok_turn2" {
		t.Fatalf("실행 순서 불일치: %v", calls)
	}
}
