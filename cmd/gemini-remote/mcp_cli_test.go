package main

import (
	"testing"

	"github.com/sam/gemini-remote-linux/internal/config"
)

func TestApplyMCPSetArgs(t *testing.T) {
	cfg := &config.Config{
		DeviceID:   "dev-local",
		DeviceName: "linux-box",
	}

	changed, setModel, err := applyMCPSetArgs(cfg, []string{
		"--model", "gemini-3-pro",
		"--thinking=high",
		"--device", "dev-other",
		"--agent-task", "false",
		"--temporary=true",
	})
	if err != nil {
		t.Fatalf("applyMCPSetArgs 실패: %v", err)
	}
	if !changed || setModel != "gemini-3-pro" {
		t.Fatalf("예상 changed=true, setModel=gemini-3-pro, got=%v, %q", changed, setModel)
	}

	b := cfg.GetMCPBridge()
	if b.DefaultModel != "gemini-3-pro" || b.DefaultThinkingLevel != "high" || b.DefaultDeviceID != "dev-other" {
		t.Fatalf("설정 값 불일치: %+v", b)
	}
	if b.EffectiveAgentTask() {
		t.Fatal("agent_task가 false여야 합니다")
	}
	if !b.DefaultTemporary {
		t.Fatal("temporary가 true여야 합니다")
	}

	// auto / local / reset 동작 검증
	_, _, err = applyMCPSetArgs(cfg, []string{"--model", "auto", "--thinking", "auto", "--device", "local"})
	if err != nil {
		t.Fatalf("auto/local 해제 실패: %v", err)
	}
	b = cfg.GetMCPBridge()
	if b.DefaultModel != "" || b.DefaultThinkingLevel != "" || b.DefaultDeviceID != "" {
		t.Fatalf("auto/local 해제 실패: %+v", b)
	}

	_, _, err = applyMCPSetArgs(cfg, []string{"--reset"})
	if err != nil {
		t.Fatalf("reset 실패: %v", err)
	}
	b = cfg.GetMCPBridge()
	if !b.EffectiveAgentTask() || b.DefaultTemporary {
		t.Fatalf("reset 후 기본값 불일치: %+v", b)
	}
}

func TestApplyMCPSetArgsValidation(t *testing.T) {
	cfg := &config.Config{}
	if _, _, err := applyMCPSetArgs(cfg, []string{"--thinking", "ultra"}); err == nil {
		t.Fatal("잘못된 thinking level에 오류가 발생해야 합니다")
	}
	if _, _, err := applyMCPSetArgs(cfg, []string{"--agent-task", "maybe"}); err == nil {
		t.Fatal("잘못된 bool 값에 오류가 발생해야 합니다")
	}
}
