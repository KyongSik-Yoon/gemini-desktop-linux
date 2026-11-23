package main

import (
	"context"
	"encoding/json"
	"fmt"
	"net"
	"net/http"
	"os"
	"strings"
	"time"

	"github.com/sam/gemini-remote-linux/internal/auth"
	"github.com/sam/gemini-remote-linux/internal/config"
	"github.com/sam/gemini-remote-linux/internal/daemon"
	"github.com/sam/gemini-remote-linux/internal/punctual"
)

type doctorStatus string

const (
	doctorPass doctorStatus = "PASS"
	doctorWarn doctorStatus = "WARN"
	doctorFail doctorStatus = "FAIL"
)

type doctorCheck struct {
	Name      string       `json:"name"`
	Status    doctorStatus `json:"status"`
	LatencyMs int64        `json:"latency_ms"`
	Detail    string       `json:"detail"`
	Hint      string       `json:"hint,omitempty"`
}

type doctorReport struct {
	OK     bool          `json:"ok"`
	Checks []doctorCheck `json:"checks"`
}

// diagnoseErrorHint는 gRPC/HTTP 오류 문구에서 가장 유력한 원인과 1차 조치 힌트를 분류한다.
func diagnoseErrorHint(checkName string, err error) string {
	if err == nil {
		return ""
	}
	low := strings.ToLower(err.Error())
	switch {
	case strings.Contains(low, "unauthenticated") ||
		strings.Contains(low, "401") ||
		strings.Contains(low, "invalid_grant") ||
		strings.Contains(low, "토큰"):
		return "OAuth 토큰 만료/누락 가능성이 높습니다. 'gemini-remote login'을 다시 실행해 주세요."
	case strings.Contains(low, "invalid_argument") ||
		strings.Contains(low, "clienttype") ||
		strings.Contains(low, "400"):
		return "서버의 RequestMetadata 또는 app_version 요구사항 변경이 의심됩니다. docs/UPSTREAM_SYNC.md를 참고하세요."
	case strings.Contains(low, "unimplemented") || strings.Contains(low, "404"):
		return "원격 RPC 경로 또는 엔드포인트 변경이 의심됩니다. 신버전 앱 프로토콜 Diff가 필요합니다."
	case strings.Contains(low, "permission_denied") || strings.Contains(low, "403"):
		return "OAuth 스코프 부족 또는 계정 권한 제한이 의심됩니다. 'gemini-remote login'으로 스코프를 재동의해 주세요."
	case strings.Contains(low, "deadline") || strings.Contains(low, "timeout") || strings.Contains(low, "no such host"):
		return "네트워크 연결 또는 프록시 설정을 확인해 주세요."
	default:
		if checkName == "punctual_choose_server" {
			return "Signaler 엔드포인트/헤더 변경 여부를 GEMINI_DEBUG_WIRE=1로 확인해 주세요."
		}
		return "상세 와이어 확인이 필요하면 GEMINI_DEBUG_WIRE=1 환경변수로 재시도해 주세요."
	}
}

func runDoctorCheck(name string, fn func(context.Context) (doctorStatus, string, string)) doctorCheck {
	t0 := time.Now()
	checkCtx, cancel := context.WithTimeout(context.Background(), 15*time.Second)
	defer cancel()
	status, detail, hint := fn(checkCtx)
	return doctorCheck{
		Name:      name,
		Status:    status,
		LatencyMs: time.Since(t0).Milliseconds(),
		Detail:    detail,
		Hint:      hint,
	}
}

func handleDoctor(ctx context.Context, cfg *config.Config, authMgr *auth.Manager, args []string) {
	jsonMode := false
	for _, a := range args {
		if a == "--json" {
			jsonMode = true
		}
	}

	checks := make([]doctorCheck, 0, 7)
	d := daemon.New(cfg, authMgr)
	robinCli := d.RobinClient()

	// 1. 설정 및 OAuth 자격증명 확인
	checks = append(checks, runDoctorCheck("config_and_credentials", func(_ context.Context) (doctorStatus, string, string) {
		hasCreds := strings.TrimSpace(cfg.OAuthClientID) != "" && strings.TrimSpace(cfg.OAuthClientSecret) != ""
		hasDevice := strings.TrimSpace(cfg.DeviceID) != ""
		if !hasCreds {
			return doctorWarn,
				fmt.Sprintf("설정 로드됨 (연결 폴더 %d개, 사용자 지정 OAuth 키 미설정 — 내장 기본값 사용)", len(cfg.ConnectedFolders)),
				"필요 시 'gemini-remote extract-keys <Gemini.app>'로 자격증명을 갱신할 수 있습니다."
		}
		if !hasDevice {
			return doctorFail, "기기 식별자가 비어 있습니다.", "설정 파일을 확인하거나 재초기화해 주세요."
		}
		return doctorPass, fmt.Sprintf("설정 및 OAuth 자격증명 정상 (연결 폴더 %d개)", len(cfg.ConnectedFolders)), ""
	}))

	// 2. OAuth 토큰 유효성 및 갱신 점검
	tokenOK := false
	checks = append(checks, runDoctorCheck("oauth_token", func(c context.Context) (doctorStatus, string, string) {
		tok, err := authMgr.AccessToken(c)
		if err != nil {
			return doctorFail, fmt.Sprintf("토큰 획득 실패: %v", err), diagnoseErrorHint("oauth_token", err)
		}
		if strings.TrimSpace(tok) == "" {
			return doctorFail, "빈 액세스 토큰 반환됨", "gemini-remote login을 다시 실행해 주세요."
		}
		tokenOK = true
		return doctorPass, "액세스 토큰 유효 (갱신 확인 완료)", ""
	}))

	if tokenOK {
		// 3. Robin GetUserStatus (모델/버전 호환성 확인)
		checks = append(checks, runDoctorCheck("robin_user_status", func(c context.Context) (doctorStatus, string, string) {
			res, err := robinCli.GetUserStatus(c)
			if err != nil {
				return doctorFail, fmt.Sprintf("GetUserStatus 실패: %v", err), diagnoseErrorHint("robin_user_status", err)
			}
			pickerCnt := 0
			for _, m := range res.Models {
				if m.InPicker {
					pickerCnt++
				}
			}
			if len(res.Models) == 0 {
				return doctorWarn, "응답은 성공했으나 모델 목록이 비어 있습니다 (스키마 변경 가능성)", "GEMINI_DEBUG_WIRE=1로 GetUserStatus 응답 필드를 점검하세요."
			}
			return doctorPass, fmt.Sprintf("정상 (모델 %d종, 선택기 표시 %d종)", len(res.Models), pickerCnt), ""
		}))

		// 4. Robin ListConversations (읽기 전용 1건 조회)
		checks = append(checks, runDoctorCheck("robin_conversations", func(c context.Context) (doctorStatus, string, string) {
			res, err := robinCli.ListConversations(c, 1, "")
			if err != nil {
				return doctorFail, fmt.Sprintf("ListConversations 실패: %v", err), diagnoseErrorHint("robin_conversations", err)
			}
			return doctorPass, fmt.Sprintf("정상 (샘플 조회 %d건)", len(res.Conversations)), ""
		}))

		// 5. GeminiNotifyService ListDevices (원격 기기 등록 상태 확인)
		checks = append(checks, runDoctorCheck("robin_devices", func(c context.Context) (doctorStatus, string, string) {
			devs, err := robinCli.ListDevices(c)
			if err != nil {
				return doctorFail, fmt.Sprintf("ListDevices 실패: %v", err), diagnoseErrorHint("robin_devices", err)
			}
			registeredSelf := false
			for _, dev := range devs {
				if strings.EqualFold(dev.DeviceID, cfg.DeviceID) {
					registeredSelf = true
					break
				}
			}
			if !registeredSelf {
				return doctorWarn,
					fmt.Sprintf("등록 기기 %d대 중 현재 PC 미등록", len(devs)),
					"폰에서 이 PC를 원격 제어하려면 'gemini-remote register'를 실행하세요."
			}
			return doctorPass, fmt.Sprintf("정상 (등록 기기 %d대, 현재 PC 등록 확인됨)", len(devs)), ""
		}))

		// 6. Punctual chooseServer 엔드포인트 확인
		checks = append(checks, runDoctorCheck("punctual_choose_server", func(c context.Context) (doctorStatus, string, string) {
			w := punctual.NewWatcher(authMgr, cfg.DeviceID)
			if err := w.ProbeServer(c); err != nil {
				return doctorFail, fmt.Sprintf("chooseServer 실패: %v", err), diagnoseErrorHint("punctual_choose_server", err)
			}
			return doctorPass, "Signaler Punctual 서버 세션 협상 정상", ""
		}))
	}

	// 7. 로컬 데몬 Unix 소켓 헬스체크 (미실행 시 WARN)
	checks = append(checks, runDoctorCheck("local_daemon_socket", func(c context.Context) (doctorStatus, string, string) {
		sockPath, err := config.SocketPath()
		if err != nil || sockPath == "" {
			return doctorWarn, "소켓 경로를 결정할 수 없습니다.", ""
		}
		if _, err := os.Stat(sockPath); err != nil {
			return doctorWarn, "로컬 데몬 소켓 미감지 (데몬 중지 상태)", "백그라운드 원격 연동·GUI를 쓰려면 'systemctl --user start gemini-remote.service'를 실행하세요."
		}
		cli := &http.Client{
			Timeout: 3 * time.Second,
			Transport: &http.Transport{
				DialContext: func(dctx context.Context, _, _ string) (net.Conn, error) {
					var d net.Dialer
					return d.DialContext(dctx, "unix", sockPath)
				},
			},
		}
		req, err := http.NewRequestWithContext(c, http.MethodGet, "http://unix/health", nil)
		if err != nil {
			return doctorFail, fmt.Sprintf("요청 생성 실패: %v", err), ""
		}
		resp, err := cli.Do(req)
		if err != nil {
			return doctorWarn, fmt.Sprintf("소켓 파일은 있으나 응답 없음: %v", err), "데몬을 재시작해 주세요: systemctl --user restart gemini-remote.service"
		}
		defer resp.Body.Close()
		var payload struct {
			OK bool `json:"ok"`
		}
		if err := json.NewDecoder(resp.Body).Decode(&payload); err != nil || !payload.OK {
			return doctorFail, fmt.Sprintf("/health 비정상 응답 (HTTP %d)", resp.StatusCode), "데몬 로그(journalctl --user -u gemini-remote)를 확인하세요."
		}
		return doctorPass, "로컬 IPC 소켓(/health) 응답 정상", ""
	}))

	_ = ctx
	allOK := true
	for _, ch := range checks {
		if ch.Status == doctorFail {
			allOK = false
			break
		}
	}

	report := doctorReport{
		OK:     allOK,
		Checks: checks,
	}

	if jsonMode {
		enc, _ := json.MarshalIndent(report, "", "  ")
		fmt.Println(string(enc))
	} else {
		fmt.Println("gemini-remote 프로토콜·연결 자가 진단")
		fmt.Println("----------------------------------------")
		for _, ch := range checks {
			fmt.Printf("[%s] %-24s (%4dms) %s\n", ch.Status, ch.Name, ch.LatencyMs, ch.Detail)
			if ch.Hint != "" {
				fmt.Printf("       ↳ 힌트: %s\n", ch.Hint)
			}
		}
		fmt.Println("----------------------------------------")
		if allOK {
			fmt.Println("결과: 핵심 프로토콜 및 인증 점검 통과")
		} else {
			fmt.Println("결과: 실패 항목이 감지되었습니다. 위 힌트와 docs/UPSTREAM_SYNC.md를 확인하세요.")
		}
	}

	if !allOK {
		os.Exit(1)
	}
}
