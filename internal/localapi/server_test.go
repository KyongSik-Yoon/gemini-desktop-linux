package localapi

import (
	"context"
	"encoding/json"
	"net"
	"net/http"
	"path/filepath"
	"testing"
	"time"

	"github.com/sam/gemini-remote-linux/internal/config"
)

func TestLocalAPIServerHealth(t *testing.T) {
	sock := filepath.Join(t.TempDir(), "test.sock")
	cfg := &config.Config{
		DeviceID:   "TEST-DEV-ID",
		DeviceName: "TestLinux",
	}
	srv := NewServer(cfg, nil, sock)

	ctx, cancel := context.WithCancel(context.Background())
	defer cancel()
	errCh := make(chan error, 1)
	go func() {
		errCh <- srv.Serve(ctx)
	}()

	cli := &http.Client{
		Transport: &http.Transport{
			DialContext: func(ctx context.Context, _, _ string) (net.Conn, error) {
				return net.Dial("unix", sock)
			},
		},
		Timeout: 2 * time.Second,
	}

	var resp *http.Response
	var err error
	for i := 0; i < 20; i++ {
		resp, err = cli.Get("http://unix/health")
		if err == nil {
			break
		}
		time.Sleep(25 * time.Millisecond)
	}
	if err != nil {
		t.Fatalf("/health 호출 실패: %v", err)
	}
	defer resp.Body.Close()

	var body map[string]any
	if err := json.NewDecoder(resp.Body).Decode(&body); err != nil {
		t.Fatalf("JSON 디코딩 실패: %v", err)
	}
	if body["ok"] != true || body["device_id"] != "TEST-DEV-ID" {
		t.Fatalf("예상치 못한 응답: %+v", body)
	}

	cancel()
	if err := <-errCh; err != nil {
		t.Fatalf("서버 종료 오류: %v", err)
	}
}
