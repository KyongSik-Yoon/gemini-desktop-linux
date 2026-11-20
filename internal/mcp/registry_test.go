package mcp

import (
	"context"
	"os"
	"os/exec"
	"path/filepath"
	"testing"

	"github.com/sam/gemini-remote-linux/internal/config"
	"github.com/sam/gemini-remote-linux/internal/proto"
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

func TestJSONSchemaToProtoNestedConversion(t *testing.T) {
	raw := map[string]any{
		"type":        "object",
		"description": "검색 파라미터",
		"required":    []any{"query"},
		"properties": map[string]any{
			"query": map[string]any{
				"type":        "string",
				"description": "검색어",
			},
			"limit": map[string]any{
				"type": "integer",
			},
			"tags": map[string]any{
				"type": "array",
				"items": map[string]any{
					"type": "string",
					"enum": []any{"work", "personal"},
				},
			},
			"note": map[string]any{
				"type": []any{"string", "null"},
			},
		},
	}

	sd := jsonSchemaToProto(raw)
	if sd.Type != proto.TypeObject || sd.Description != "검색 파라미터" {
		t.Fatalf("최상위 스키마 변환 오류: %+v", sd)
	}
	if len(sd.Required) != 1 || sd.Required[0] != "query" {
		t.Fatalf("required 변환 오류: %+v", sd.Required)
	}
	if sd.Properties["limit"] == nil || sd.Properties["limit"].Type != proto.TypeInteger {
		t.Fatalf("integer 프로퍼티 변환 오류: %+v", sd.Properties["limit"])
	}
	tags := sd.Properties["tags"]
	if tags == nil || tags.Type != proto.TypeArray || tags.Items == nil || len(tags.Items.Enum) != 2 {
		t.Fatalf("중첩 array/enum 변환 오류: %+v", tags)
	}
	note := sd.Properties["note"]
	if note == nil || note.Type != proto.TypeString || !note.Nullable {
		t.Fatalf("nullable 타입 배열 변환 오류: %+v", note)
	}
}

func TestExternalMCPPersistentStdioSession(t *testing.T) {
	py, err := exec.LookPath("python3")
	if err != nil {
		t.Skip("python3 미설치 환경")
	}
	script := filepath.Join(t.TempDir(), "mock_mcp.py")
	code := `import sys, json, os
for line in sys.stdin:
    line = line.strip()
    if not line:
        continue
    msg = json.loads(line)
    mid = msg.get("id")
    method = msg.get("method")
    if method == "initialize":
        resp = {"jsonrpc": "2.0", "id": mid, "result": {"protocolVersion": "2024-11-05"}}
    elif method == "tools/list":
        resp = {"jsonrpc": "2.0", "id": mid, "result": {"tools": [{
            "name": "ping",
            "description": "pid 반환",
            "inputSchema": {
                "type": "object",
                "required": ["msg"],
                "properties": {"msg": {"type": "string", "description": "메시지"}}
            }
        }]}}
    elif method == "tools/call":
        resp = {"jsonrpc": "2.0", "id": mid, "result": {"pid": os.getpid(), "args": msg.get("params", {}).get("arguments", {})}}
    else:
        continue
    sys.stdout.write(json.dumps(resp) + "\n")
    sys.stdout.flush()
`
	if err := os.WriteFile(script, []byte(code), 0o600); err != nil {
		t.Fatalf("모의 MCP 스크립트 생성 실패: %v", err)
	}

	sc := config.MCPServerConfig{
		ServerID: "mock",
		Command:  py,
		Args:     []string{script},
		Enabled:  true,
	}
	reg := NewRegistry(&config.Config{MCPServers: []config.MCPServerConfig{sc}})
	defer reg.Close()

	decls := reg.BundledServers()
	if len(decls) != 1 || len(decls[0].ToolDeclarations) != 1 {
		t.Fatalf("외부 MCP 도구 탐색 실패: %+v", decls)
	}
	params := decls[0].ToolDeclarations[0].Parameters
	if params == nil || params.Properties["msg"] == nil || params.Properties["msg"].Type != proto.TypeString {
		t.Fatalf("외부 MCP inputSchema 변환 실패: %+v", params)
	}

	ctx := context.Background()
	res1, err := reg.callExternalMCP(ctx, sc, "ping", map[string]any{"msg": "first"})
	if err != nil {
		t.Fatalf("1차 호출 실패: %v", err)
	}
	res2, err := reg.callExternalMCP(ctx, sc, "ping", map[string]any{"msg": "second"})
	if err != nil {
		t.Fatalf("2차 호출 실패: %v", err)
	}
	if res1["pid"] != res2["pid"] {
		t.Fatalf("영속 stdio 세션이 재사용되어야 함: pid1=%v pid2=%v", res1["pid"], res2["pid"])
	}
}
