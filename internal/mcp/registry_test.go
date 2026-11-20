package mcp

import (
	"context"
	"os"
	"path/filepath"
	"testing"

	"github.com/sam/gemini-remote-linux/internal/config"
)

func TestObsidianPathTraversalBlocked(t *testing.T) {
	vault := t.TempDir()
	outsideDir := t.TempDir()
	secretPath := filepath.Join(outsideDir, "secret.md")
	if err := os.WriteFile(secretPath, []byte("top-secret"), 0o600); err != nil {
		t.Fatalf("외부 파일 생성 실패: %v", err)
	}
	if err := os.WriteFile(filepath.Join(vault, "note.md"), []byte("hello"), 0o600); err != nil {
		t.Fatalf("볼트 노트 생성 실패: %v", err)
	}

	cfg := &config.Config{
		ObsidianVaultDir: vault,
		ConnectedFolders: []string{vault},
	}
	reg := NewRegistry(cfg)
	ctx := context.Background()

	// 정상 노트 읽기
	okRes, err := reg.execObsidianTool(ctx, "obsidian_read_note", map[string]any{"note_path": "note.md"})
	if err != nil || okRes["content"] != "hello" {
		t.Fatalf("정상 노트 읽기 실패: res=%v err=%v", okRes, err)
	}

	// 상위 디렉터리 탈출 읽기 차단
	relEscape, _ := filepath.Rel(vault, secretPath)
	escRes, err := reg.execObsidianTool(ctx, "obsidian_read_note", map[string]any{"note_path": relEscape})
	if err == nil && escRes["error"] == nil {
		t.Fatalf("경로 순회 읽기가 차단되어야 함: %v", escRes)
	}

	// 허용되지 않은 임의 vault_path 차단
	if _, err := reg.execObsidianTool(ctx, "obsidian_list_notes", map[string]any{"vault_path": "/etc"}); err == nil {
		t.Fatalf("허용되지 않은 vault_path(/etc)는 차단되어야 함")
	}
}
