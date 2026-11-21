package robin

import (
	"context"
	"net/http"
	"testing"

	"github.com/sam/gemini-remote-linux/internal/proto"
)

func TestIsAllowedGoogleImageHost(t *testing.T) {
	allowed := []string{
		"lh3.googleusercontent.com",
		"LH3.GOOGLEUSERCONTENT.COM",
		"encrypted-tbn0.gstatic.com",
	}
	for _, host := range allowed {
		if !isAllowedGoogleImageHost(host) {
			t.Fatalf("허용되어야 할 호스트가 거부됨: %s", host)
		}
	}

	blocked := []string{
		"example.com",
		"googleusercontent.com.evil.test",
		"evil-gstatic.com",
		"127.0.0.1",
		"localhost",
		"",
	}
	for _, host := range blocked {
		if isAllowedGoogleImageHost(host) {
			t.Fatalf("차단되어야 할 호스트가 허용됨: %s", host)
		}
	}
}

func TestCheckGoogleImageRedirect(t *testing.T) {
	// 허용 호스트로의 HTTPS 리다이렉트는 통과하되 Authorization 헤더를 제거해야 함
	req, err := http.NewRequest(http.MethodGet, "https://lh3.googleusercontent.com/image.png", nil)
	if err != nil {
		t.Fatal(err)
	}
	req.Header.Set("Authorization", "Bearer dummy-token")
	if err := checkGoogleImageRedirect(req, []*http.Request{req}); err != nil {
		t.Fatalf("허용된 리다이렉트가 실패함: %v", err)
	}
	if got := req.Header.Get("Authorization"); got != "" {
		t.Fatalf("리다이렉트 시 Authorization 헤더가 제거되지 않음: %q", got)
	}

	// 외부 호스트 리다이렉트 차단
	evilReq, _ := http.NewRequest(http.MethodGet, "https://evil.example.com/steal", nil)
	if err := checkGoogleImageRedirect(evilReq, []*http.Request{req}); err == nil {
		t.Fatalf("외부 호스트 리다이렉트가 차단되지 않음")
	}

	// 비암호화 HTTP 리다이렉트 차단
	httpReq, _ := http.NewRequest(http.MethodGet, "http://lh3.googleusercontent.com/image.png", nil)
	if err := checkGoogleImageRedirect(httpReq, []*http.Request{req}); err == nil {
		t.Fatalf("HTTP 리다이렉트가 차단되지 않음")
	}

	// 리다이렉트 횟수 초과 차단
	tooMany := make([]*http.Request, 5)
	if err := checkGoogleImageRedirect(req, tooMany); err == nil {
		t.Fatalf("리다이렉트 5회 초과가 차단되지 않음")
	}
}

func TestFetchAvatarBytesRejectsUnsafeURLs(t *testing.T) {
	cli := &Client{}
	for _, bad := range []string{
		"",
		"http://lh3.googleusercontent.com/avatar.png",
		"https://evil.example.com/avatar.png",
		"file:///etc/passwd",
	} {
		if _, _, err := cli.FetchAvatarBytes(context.Background(), bad); err == nil {
			t.Fatalf("비허용 아바타 URL이 통과됨: %q", bad)
		}
	}
}

func TestDedupAppArtifacts(t *testing.T) {
	in := []proto.AppArtifact{
		{ID: "a1", ResponseID: "rc1", Kind: "document", ConversationID: "c1", Title: "대시보드", CreatedAtUnix: 100},
		{ID: "a1", ResponseID: "rc1", Kind: "document", ConversationID: "c1", Title: "대시보드", CreatedAtUnix: 100},
		{ID: "a2", ResponseID: "rc2", Kind: "document", ConversationID: "c1", Title: "대시보드", CreatedAtUnix: 100},
		{ID: "i1", Kind: "image", ConversationID: "c2", URL: "https://lh3.googleusercontent.com/x", Title: "img"},
		{ID: "i2", Kind: "image", ConversationID: "c2", URL: "https://lh3.googleusercontent.com/x", Title: "img"},
		{ID: "d2", ResponseID: "rc3", Kind: "document", ConversationID: "c3", Title: "다른 문서", CreatedAtUnix: 200},
	}
	got := dedupAppArtifacts(in)
	if len(got) != 3 {
		t.Fatalf("아티팩트 중복 제거 실패: 예상 3개, 실제 %d개 (%+v)", len(got), got)
	}
}
