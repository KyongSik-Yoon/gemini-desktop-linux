package localapi

import (
	"bytes"
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"testing"

	"github.com/sam/gemini-remote-linux/internal/config"
)

// TestLocalAPIContractForRustIPC는 Rust gemini-ui(ui/src/ipc.rs)가 역직렬화하는
// 핵심 엔드포인트의 JSON 키 이름과 값 타입 계약을 고정한다.
// 필드명이 변경되거나 누락되면 빌드 단계에서 즉시 회귀를 탐지한다.
func TestLocalAPIContractForRustIPC(t *testing.T) {
	dir := t.TempDir()
	st := &WorkspaceStore{
		dir:           dir,
		runningScheds: make(map[string]bool),
	}
	st.load()

	srv := &Server{
		cfg: &config.Config{
			DeviceID:   "TEST-DEVICE-0001",
			DeviceName: "test-workstation",
		},
		sockPath: "/tmp/test-gemini-remote.sock",
		store:    st,
	}

	t.Run("GET /health 계약", func(t *testing.T) {
		req := httptest.NewRequest(http.MethodGet, "/health", nil)
		rec := httptest.NewRecorder()
		srv.handleHealth(rec, req)
		if rec.Code != http.StatusOK {
			t.Fatalf("HTTP %d", rec.Code)
		}
		var m map[string]any
		if err := json.Unmarshal(rec.Body.Bytes(), &m); err != nil {
			t.Fatalf("JSON 파싱 실패: %v", err)
		}
		for _, k := range []string{"ok", "device_id", "device_name", "socket"} {
			if _, exists := m[k]; !exists {
				t.Fatalf("/health 필수 계약 키 누락: %s (body=%s)", k, rec.Body.String())
			}
		}
		if okVal, _ := m["ok"].(bool); !okVal {
			t.Fatalf("ok는 true여야 함: %+v", m["ok"])
		}
	})

	t.Run("GET/PATCH /v1/preferences 계약", func(t *testing.T) {
		patchBody := []byte(`{
			"selected_model_id": "gemini-3-pro",
			"thinking_level": 4,
			"theme_mode": "dark",
			"language": "ko",
			"personal_intelligence": true,
			"custom_instructions": "간결하게 답변",
			"close_to_tray": false,
			"sidebar_width": 320
		}`)
		patchReq := httptest.NewRequest(http.MethodPatch, "/v1/preferences", bytes.NewReader(patchBody))
		patchRec := httptest.NewRecorder()
		srv.handlePatchPreferences(patchRec, patchReq)
		if patchRec.Code != http.StatusOK {
			t.Fatalf("PATCH HTTP %d: %s", patchRec.Code, patchRec.Body.String())
		}

		getReq := httptest.NewRequest(http.MethodGet, "/v1/preferences", nil)
		getRec := httptest.NewRecorder()
		srv.handleGetPreferences(getRec, getReq)
		if getRec.Code != http.StatusOK {
			t.Fatalf("GET HTTP %d", getRec.Code)
		}
		var m map[string]any
		if err := json.Unmarshal(getRec.Body.Bytes(), &m); err != nil {
			t.Fatalf("JSON 파싱 실패: %v", err)
		}
		required := []string{
			"selected_model_id",
			"thinking_level",
			"theme_mode",
			"language",
			"personal_intelligence",
			"custom_instructions",
			"close_to_tray",
			"sidebar_width",
		}
		for _, k := range required {
			if _, exists := m[k]; !exists {
				t.Fatalf("/v1/preferences 필수 계약 키 누락: %s (body=%s)", k, getRec.Body.String())
			}
		}
		if int(m["sidebar_width"].(float64)) != 320 {
			t.Fatalf("sidebar_width 불일치: %+v", m["sidebar_width"])
		}
	})
}
