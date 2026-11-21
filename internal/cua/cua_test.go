package cua

import (
	"bytes"
	"context"
	"os/exec"
	"strings"
	"testing"
	"time"
)

func TestBuildPythonPreludeSyntaxAndHelpers(t *testing.T) {
	prelude := buildPythonPrelude("/tmp/gemini_test_shot.png")
	for _, want := range []string{
		"class _WlrVirtualPointer",
		"def move_to(",
		"def click(",
		"def double_click(",
		"def right_click(",
		"def drag_to(",
		"def scroll(",
		"def hotkey(",
		"def open_app(",
	} {
		if !strings.Contains(prelude, want) {
			t.Fatalf("파이썬 프렐류드에 필수 심볼 누락: %s", want)
		}
	}

	if _, err := exec.LookPath("python3"); err != nil {
		t.Skip("python3 미설치 환경")
	}
	ctx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
	defer cancel()
	cmd := exec.CommandContext(ctx, "python3", "-c", "import ast, sys; ast.parse(sys.stdin.read())")
	cmd.Stdin = bytes.NewBufferString(prelude)
	if out, err := cmd.CombinedOutput(); err != nil {
		t.Fatalf("파이썬 프렐류드 문법 오류: %v\n%s", err, string(out))
	}
}

func TestRunUIControlSimpleScript(t *testing.T) {
	if _, err := exec.LookPath("python3"); err != nil {
		t.Skip("python3 미설치 환경")
	}
	ctx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
	defer cancel()
	res, err := RunUIControl(ctx, `print("keymap:", _norm_key("cmd"), _norm_key("return"))`, false)
	if err != nil {
		t.Fatalf("RunUIControl 실행 실패: %v", err)
	}
	if !strings.Contains(res.Stdout, "keymap:") || !strings.Contains(res.Stdout, "Return") {
		t.Fatalf("키 매핑 출력 불일치: stdout=%q stderr=%q", res.Stdout, res.Stderr)
	}
}
