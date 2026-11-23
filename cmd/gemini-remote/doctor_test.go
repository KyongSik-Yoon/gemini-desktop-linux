package main

import (
	"errors"
	"strings"
	"testing"
)

func TestDiagnoseErrorHint(t *testing.T) {
	cases := []struct {
		name      string
		checkName string
		err       error
		wantSub   string
	}{
		{
			name:      "OAuth 만료",
			checkName: "oauth_token",
			err:       errors.New("rpc error: code = Unauthenticated"),
			wantSub:   "gemini-remote login",
		},
		{
			name:      "메타데이터/버전 불일치",
			checkName: "robin_user_status",
			err:       errors.New("INVALID_ARGUMENT: ClientType is UNSPECIFIED"),
			wantSub:   "UPSTREAM_SYNC.md",
		},
		{
			name:      "RPC 미구현",
			checkName: "robin_conversations",
			err:       errors.New("gRPC status=12 UNIMPLEMENTED"),
			wantSub:   "Diff",
		},
		{
			name:      "Punctual 기본 힌트",
			checkName: "punctual_choose_server",
			err:       errors.New("unexpected wire EOF"),
			wantSub:   "GEMINI_DEBUG_WIRE=1",
		},
	}
	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			got := diagnoseErrorHint(tc.checkName, tc.err)
			if !strings.Contains(got, tc.wantSub) {
				t.Fatalf("힌트에 %q가 포함되어야 함: got=%q", tc.wantSub, got)
			}
		})
	}
}
