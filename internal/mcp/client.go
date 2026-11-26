package mcp

import (
	"context"
	"net"
	"net/http"
)

// newDaemonUnixHTTPClient는 로컬 데몬 Unix 소켓 SSE 구독용 공용 HTTP 클라이언트를 생성한다.
func newDaemonUnixHTTPClient(sockPath string) *http.Client {
	transport := &http.Transport{
		DialContext: func(dctx context.Context, _, _ string) (net.Conn, error) {
			var d net.Dialer
			return d.DialContext(dctx, "unix", sockPath)
		},
	}
	return &http.Client{Transport: transport}
}
