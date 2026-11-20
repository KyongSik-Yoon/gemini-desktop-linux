package tools

import (
	"context"
	"os"
	"path/filepath"
	"strings"
	"sync"
	"testing"

	"github.com/sam/gemini-remote-linux/internal/config"
)

func TestCappedBufferAndBgCommandRaceFree(t *testing.T) {
	cfg := &config.Config{ConnectedFolders: []string{t.TempDir()}}
	ex := NewExecutor(cfg, nil, nil, nil)

	ctx := context.Background()
	res, err := ex.toolRunCommand(ctx, map[string]any{
		"command":    "for i in $(seq 1 5); do echo line-$i; sleep 0.05; done",
		"timeout_ms": 80,
	})
	if err != nil {
		t.Fatalf("toolRunCommand 실패: %v", err)
	}
	cmdID, _ := res["command_id"].(string)
	if cmdID != "" {
		var wg sync.WaitGroup
		for i := 0; i < 4; i++ {
			wg.Add(1)
			go func() {
				defer wg.Done()
				_, _ = ex.toolCommandStatus(ctx, map[string]any{
					"command_id":   cmdID,
					"wait_seconds": 1,
				})
			}()
		}
		wg.Wait()
	}
}

func TestReadSkillPathTraversalBlocked(t *testing.T) {
	cfg := &config.Config{}
	ex := NewExecutor(cfg, nil, nil, nil)
	for _, bad := range []string{"../secret", "../../etc/passwd", "a/b", "..", "."} {
		if _, err := ex.toolReadSkill(map[string]any{"name": bad}); err == nil {
			t.Fatalf("경로 순회 스킬 이름 %q가 차단되어야 함", bad)
		}
	}
}

func TestReplaceFileContentGuardsEmptyOldStrAndPreservesMode(t *testing.T) {
	dir := t.TempDir()
	p := filepath.Join(dir, "script.sh")
	if err := os.WriteFile(p, []byte("#!/bin/sh\necho hi\n"), 0o750); err != nil {
		t.Fatalf("테스트 파일 생성 실패: %v", err)
	}
	ex := NewExecutor(&config.Config{}, nil, nil, nil)
	if _, err := ex.toolReplaceFileContent(map[string]any{
		"path":    p,
		"old_str": "",
		"new_str": "x",
	}); err == nil {
		t.Fatalf("빈 old_str는 에러를 반환해야 함")
	}
	if _, err := ex.toolReplaceFileContent(map[string]any{
		"path":    p,
		"old_str": "hi",
		"new_str": "hello",
	}); err != nil {
		t.Fatalf("치환 실패: %v", err)
	}
	fi, err := os.Stat(p)
	if err != nil {
		t.Fatalf("stat 실패: %v", err)
	}
	if fi.Mode().Perm() != 0o750 {
		t.Fatalf("기존 권한 0750이 유지되어야 함: got %o", fi.Mode().Perm())
	}
}

func TestTruncateStrUTF8Safe(t *testing.T) {
	s := strings.Repeat("가", 10) // 30 bytes
	got := truncateStr(s, 5)
	if !strings.HasPrefix(got, "가\n...") {
		t.Fatalf("UTF-8 경계 보존 절단 실패: %q", got)
	}
}
