package daemon

import (
	"context"
	"sync"
	"testing"
	"time"

	"github.com/sam/gemini-remote-linux/internal/config"
	"github.com/sam/gemini-remote-linux/internal/proto"
	"github.com/sam/gemini-remote-linux/internal/robin"
)

func TestSubscribeUnsubscribeAndRecordToolEventNoPanic(t *testing.T) {
	d := &Daemon{
		cfg:            &config.Config{},
		activeSessions: make(map[string]*sessionEntry),
		lastHintProbe:  make(map[string]time.Time),
		subscribers:    make(map[chan robin.ToolExecEvent]struct{}),
	}

	var wg sync.WaitGroup
	for i := 0; i < 8; i++ {
		wg.Add(1)
		go func() {
			defer wg.Done()
			for j := 0; j < 200; j++ {
				_, unsub := d.SubscribeRemoteEvents()
				d.recordToolEvent(robin.ToolExecEvent{
					ToolName: "run_command",
					Phase:    "started",
				})
				unsub()
			}
		}()
	}
	wg.Wait()
}

func TestPendingWakeUpSurvivesToolCall(t *testing.T) {
	d := &Daemon{
		cfg:            &config.Config{},
		activeSessions: make(map[string]*sessionEntry),
		lastHintProbe:  make(map[string]time.Time),
		subscribers:    make(map[chan robin.ToolExecEvent]struct{}),
	}

	var mu sync.Mutex
	var tokens []string
	d.sessionRunner = func(ctx context.Context, routingKey, routingToken string, hooks robin.SessionHooks) error {
		mu.Lock()
		tokens = append(tokens, routingToken)
		callNum := len(tokens)
		mu.Unlock()
		if callNum == 1 {
			hooks.OnToolCallStart()
			// 도구 실행 도중 후속 WakeUp 도착 시뮬레이션
			d.handleWakeUpEvent(ctx, &proto.WakeUpEvent{
				SessionRoutingKey: routingKey,
				RoutingToken:      "tok-2",
			})
			hooks.OnToolCallEnd()
		}
		return nil
	}

	d.handleWakeUpEvent(context.Background(), &proto.WakeUpEvent{
		SessionRoutingKey: "rk-1",
		RoutingToken:      "tok-1",
	})

	mu.Lock()
	defer mu.Unlock()
	if len(tokens) != 2 || tokens[0] != "tok-1" || tokens[1] != "tok-2" {
		t.Fatalf("도구 실행 중 도착한 후속 WakeUp이 승계되어야 함: got %v", tokens)
	}
}
