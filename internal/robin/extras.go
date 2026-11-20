package robin

import (
	"context"
	"fmt"
	"io"
	"net/http"
	"net/url"
	"sort"
	"strings"
	"sync"

	"github.com/sam/gemini-remote-linux/internal/proto"
)

const (
	PathListGeminiNotebooks      = "/assistant.mobile.robin.RobinService/ListGeminiNotebooks"
	PathCreateGbhNotebook        = "/assistant.mobile.robin.RobinService/CreateGbhNotebook"
	PathListRemySchedules        = "/assistant.mobile.robin.RobinService/ListRemySchedules"
	PathListRecommendedSchedules = "/assistant.mobile.robin.RobinService/ListRecommendedSchedules"
	PathDeleteRemySchedule       = "/assistant.mobile.robin.RobinService/DeleteRemySchedule"
	PathExecuteRemySchedule      = "/assistant.mobile.robin.RobinService/ExecuteRemySchedule"
	PathAddRecommendedSchedule   = "/assistant.mobile.robin.RobinService/AddRecommendedSchedule"
	PathListGeminiAppArtifacts   = "/assistant.mobile.robin.RobinService/ListGeminiAppArtifacts"
	PathGetUsageInfo             = "/assistant.mobile.robin.RobinService/GetUsageInfo"
	PathCreateSharedConversation = "/assistant.mobile.robin.RobinService/CreateSharedConversation"
	PathWriteFeedback            = "/assistant.mobile.robin.RobinService/WriteFeedback"
)

// ListGeminiNotebooks는 서버에 저장된 사용자의 Gemini 노트북 목록을 조회한다.
func (c *Client) ListGeminiNotebooks(ctx context.Context) ([]proto.RemoteNotebook, error) {
	reqBytes := proto.EncodeListGeminiNotebooksRequest(c.buildChatMetadata())
	respBytes, err := c.invokeUnary(ctx, PathListGeminiNotebooks, reqBytes)
	if err != nil {
		return nil, err
	}
	return proto.DecodeListGeminiNotebooksResponse(respBytes)
}

// CreateGbhNotebook은 서버에 새 Gemini 노트북 생성을 시도한다.
func (c *Client) CreateGbhNotebook(ctx context.Context, title string) (*proto.RemoteNotebook, error) {
	reqBytes := proto.EncodeCreateGbhNotebookRequest(c.buildChatMetadata(), title)
	respBytes, err := c.invokeUnary(ctx, PathCreateGbhNotebook, reqBytes)
	if err != nil {
		return nil, err
	}
	return proto.DecodeCreateGbhNotebookResponse(respBytes)
}

// ListRemySchedules는 서버에 등록된 예약 작업 목록을 조회한다.
func (c *Client) ListRemySchedules(ctx context.Context) ([]proto.RemoteSchedule, error) {
	reqBytes := proto.EncodeListRemySchedulesRequest(c.buildChatMetadata())
	respBytes, err := c.invokeUnary(ctx, PathListRemySchedules, reqBytes)
	if err != nil {
		return nil, err
	}
	return proto.DecodeListRemySchedulesResponse(respBytes)
}

// ListRecommendedSchedules는 서버가 제안하는 추천 예약 작업 목록을 조회한다.
func (c *Client) ListRecommendedSchedules(ctx context.Context) ([]proto.RecommendedSchedule, error) {
	reqBytes := proto.EncodeListRecommendedSchedulesRequest(c.buildChatMetadata())
	respBytes, err := c.invokeUnary(ctx, PathListRecommendedSchedules, reqBytes)
	if err != nil {
		return nil, err
	}
	return proto.DecodeListRecommendedSchedulesResponse(respBytes)
}

// DeleteRemySchedule은 서버 예약 작업을 삭제한다.
func (c *Client) DeleteRemySchedule(ctx context.Context, scheduleID string) error {
	reqBytes := proto.EncodeDeleteRemyScheduleRequest(c.buildChatMetadata(), scheduleID)
	_, err := c.invokeUnary(ctx, PathDeleteRemySchedule, reqBytes)
	return err
}

// ExecuteRemySchedule은 서버 예약 작업을 즉시 실행한다.
func (c *Client) ExecuteRemySchedule(ctx context.Context, scheduleID string) error {
	reqBytes := proto.EncodeExecuteRemyScheduleRequest(c.buildChatMetadata(), scheduleID)
	_, err := c.invokeUnary(ctx, PathExecuteRemySchedule, reqBytes)
	return err
}

// AddRecommendedSchedule은 추천 예약 템플릿을 내 예약 작업으로 추가한다.
func (c *Client) AddRecommendedSchedule(ctx context.Context, templateID string) error {
	reqBytes := proto.EncodeAddRecommendedScheduleRequest(c.buildChatMetadata(), templateID)
	_, err := c.invokeUnary(ctx, PathAddRecommendedSchedule, reqBytes)
	return err
}

// ListGeminiAppArtifacts는 서버의 문서(Canvas)와 미디어(이미지) 아티팩트를 병렬로 조회해 병합한다.
func (c *Client) ListGeminiAppArtifacts(ctx context.Context, pageSize int32) ([]proto.AppArtifact, error) {
	if pageSize <= 0 {
		pageSize = 24
	}
	var (
		wg        sync.WaitGroup
		docs      []proto.AppArtifact
		media     []proto.AppArtifact
		docsErr   error
		mediaErr  error
	)
	wg.Add(2)
	go func() {
		defer wg.Done()
		reqBytes := proto.EncodeListGeminiAppArtifactsRequest(c.buildChatMetadata(), pageSize, false)
		respBytes, err := c.invokeUnary(ctx, PathListGeminiAppArtifacts, reqBytes)
		if err != nil {
			docsErr = err
			return
		}
		docs, docsErr = proto.DecodeListGeminiAppArtifactsResponse(respBytes, false)
	}()
	go func() {
		defer wg.Done()
		reqBytes := proto.EncodeListGeminiAppArtifactsRequest(c.buildChatMetadata(), pageSize, true)
		respBytes, err := c.invokeUnary(ctx, PathListGeminiAppArtifacts, reqBytes)
		if err != nil {
			mediaErr = err
			return
		}
		media, mediaErr = proto.DecodeListGeminiAppArtifactsResponse(respBytes, true)
	}()
	wg.Wait()

	if docsErr != nil && mediaErr != nil {
		return nil, docsErr
	}
	merged := make([]proto.AppArtifact, 0, len(docs)+len(media))
	merged = append(merged, docs...)
	merged = append(merged, media...)
	sort.Slice(merged, func(i, j int) bool {
		return merged[i].CreatedAtUnix > merged[j].CreatedAtUnix
	})
	return merged, nil
}

// ListGeminiMediaArtifacts는 서버 라이브러리에서 생성 이미지(미디어) 아티팩트만 조회한다.
func (c *Client) ListGeminiMediaArtifacts(ctx context.Context, pageSize int32) ([]proto.AppArtifact, error) {
	if pageSize <= 0 {
		pageSize = 60
	}
	reqBytes := proto.EncodeListGeminiAppArtifactsRequest(c.buildChatMetadata(), pageSize, true)
	respBytes, err := c.invokeUnary(ctx, PathListGeminiAppArtifacts, reqBytes)
	if err != nil {
		return nil, err
	}
	return proto.DecodeListGeminiAppArtifactsResponse(respBytes, true)
}

// GetUsageInfo는 현재 계정의 사용량 한도 정보를 조회한다.
func (c *Client) GetUsageInfo(ctx context.Context) (*proto.UsageInfo, error) {
	reqBytes := proto.EncodeGetUsageInfoRequest(c.buildChatMetadata())
	respBytes, err := c.invokeUnary(ctx, PathGetUsageInfo, reqBytes)
	if err != nil {
		return nil, err
	}
	return proto.DecodeGetUsageInfoResponse(respBytes)
}

// CreateSharedConversation은 대화방 공유 링크를 생성한다.
func (c *Client) CreateSharedConversation(ctx context.Context, conversationID, title string) (string, error) {
	reqBytes := proto.EncodeCreateSharedConversationRequest(c.buildChatMetadata(), conversationID, title)
	respBytes, err := c.invokeUnary(ctx, PathCreateSharedConversation, reqBytes)
	if err != nil {
		return "", err
	}
	return proto.DecodeCreateSharedConversationResponse(respBytes)
}

// WriteFeedback은 특정 모델 응답에 대한 좋아요/싫어요 피드백을 전송한다.
func (c *Client) WriteFeedback(ctx context.Context, conversationID, requestID, responseID string, positive bool, comment string) error {
	reqBytes := proto.EncodeWriteFeedbackRequest(c.buildChatMetadata(), conversationID, requestID, responseID, positive, comment)
	_, err := c.invokeUnary(ctx, PathWriteFeedback, reqBytes)
	return err
}

// MarkConversationUnread는 대화방의 읽지 않음 상태를 서버에 반영 시도한다.
func (c *Client) MarkConversationUnread(ctx context.Context, conversationID string, unread bool) error {
	reqBytes := proto.EncodeUpdateConversationUnreadRequest(c.buildChatMetadata(), conversationID, unread)
	_, err := c.invokeUnary(ctx, PathUpdateConversation, reqBytes)
	return err
}

func isAllowedGoogleImageHost(host string) bool {
	h := strings.ToLower(host)
	return strings.HasSuffix(h, ".googleusercontent.com") ||
		h == "lh3.googleusercontent.com" ||
		strings.HasSuffix(h, ".gstatic.com")
}

func checkGoogleImageRedirect(req *http.Request, via []*http.Request) error {
	if len(via) >= 5 {
		return fmt.Errorf("이미지 리다이렉트 횟수 초과")
	}
	if req.URL.Scheme != "https" || !isAllowedGoogleImageHost(req.URL.Hostname()) {
		return fmt.Errorf("허용되지 않는 리다이렉트 대상입니다: %s", req.URL.Hostname())
	}
	// 리다이렉트 요청에는 OAuth 토큰을 전파하지 않음
	req.Header.Del("Authorization")
	return nil
}

// FetchRemoteImageBytes는 허용된 Google 이미지 호스트에서 썸네일 바이트를 안전하게 다운로드한다.
func (c *Client) FetchRemoteImageBytes(ctx context.Context, rawURL string) ([]byte, string, error) {
	parsed, err := url.Parse(strings.TrimSpace(rawURL))
	if err != nil || parsed.Scheme != "https" {
		return nil, "", fmt.Errorf("허용되지 않는 이미지 URL입니다")
	}
	if !isAllowedGoogleImageHost(parsed.Hostname()) {
		return nil, "", fmt.Errorf("허용되지 않는 이미지 호스트입니다: %s", parsed.Hostname())
	}

	req, err := http.NewRequestWithContext(ctx, http.MethodGet, parsed.String(), nil)
	if err != nil {
		return nil, "", err
	}
	if token, terr := c.authMgr.AccessToken(ctx); terr == nil && token != "" {
		req.Header.Set("Authorization", "Bearer "+token)
	}
	resp, err := c.getImageClient().Do(req)
	if err != nil {
		return nil, "", err
	}
	defer resp.Body.Close()
	if resp.StatusCode != http.StatusOK {
		return nil, "", fmt.Errorf("이미지 다운로드 HTTP %d", resp.StatusCode)
	}
	data, err := io.ReadAll(io.LimitReader(resp.Body, 8<<20))
	if err != nil {
		return nil, "", err
	}
	ct := resp.Header.Get("Content-Type")
	if ct == "" {
		ct = "image/jpeg"
	}
	return data, ct, nil
}
