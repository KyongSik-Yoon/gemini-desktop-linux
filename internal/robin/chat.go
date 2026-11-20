package robin

import (
	"bytes"
	"context"
	"encoding/json"
	"fmt"
	"io"
	"net/http"
	"net/url"
	"os"
	"path/filepath"
	"sort"
	"strings"
	"sync"
	"time"

	"github.com/sam/gemini-remote-linux/internal/proto"
)

const (
	PathGetUserStatus         = "/assistant.mobile.robin.RobinService/GetUserStatus"
	PathListConversations     = "/assistant.mobile.robin.RobinService/ListConversations"
	PathListConversationTurns = "/assistant.mobile.robin.RobinService/ListConversationTurns"
	PathListBots              = "/assistant.mobile.robin.RobinService/ListBots"
	PathSearchConversations   = "/assistant.mobile.robin.RobinService/SearchConversationHistory"
	PathProcessQuery          = "/assistant.mobile.robin.RobinService/ProcessQuery"
	PathAbortGeneration       = "/assistant.mobile.robin.RobinService/AbortGeneration"
	PathDeleteConversation    = "/assistant.mobile.robin.RobinService/DeleteConversation"
	PathUpdateConversation    = "/assistant.mobile.robin.RobinService/UpdateConversation"
)

// UserProfile은 로그인된 Google 계정의 프로필 및 Gemini 구독 티어 정보다.
type UserProfile struct {
	Name       string `json:"name"`
	GivenName  string `json:"given_name,omitempty"`
	Email      string `json:"email"`
	Picture    string `json:"picture,omitempty"`
	Tier       string `json:"tier,omitempty"`
	DeviceName string `json:"device_name,omitempty"`
	DeviceID   string `json:"device_id,omitempty"`
}

func (c *Client) buildChatMetadata() []byte {
	locale := c.cfg.Locale
	if locale == "" {
		locale = "ko-KR"
	}
	tz := c.cfg.TimeZone
	if tz == "" {
		tz = "Asia/Seoul"
	}
	return proto.EncodeChatRequestMetadata(locale, tz, strings.ToUpper(c.cfg.DeviceID))
}

// GetUserStatus는 현재 계정의 사용 가능한 Gemini 모델 목록과 피커 구성을 조회한다(60초 TTL 캐시 및 동시 호출 병합).
func (c *Client) GetUserStatus(ctx context.Context) (*proto.UserStatusResult, error) {
	return c.userStatusCache.GetOrDo(60*time.Second, func() (*proto.UserStatusResult, error) {
		reqBytes := proto.EncodeGetUserStatusRequest(c.buildChatMetadata())
		respBytes, err := c.invokeUnary(ctx, PathGetUserStatus, reqBytes)
		if err != nil {
			return nil, err
		}
		return proto.DecodeGetUserStatusResponse(respBytes)
	})
}

// GetUserProfile은 OAuth userinfo와 GetUserStatus를 결합해 프로필 이름·이메일·아바타 URL·구독 티어를 반환한다(60초 TTL 캐시 및 동시 호출 병합).
func (c *Client) GetUserProfile(ctx context.Context) (*UserProfile, error) {
	return c.userProfileCache.GetOrDo(60*time.Second, func() (*UserProfile, error) {
		token, err := c.authMgr.AccessToken(ctx)
		if err != nil {
			return nil, err
		}

		prof := &UserProfile{
			DeviceName: c.cfg.DeviceName,
			DeviceID:   c.cfg.DeviceID,
		}

		var wg sync.WaitGroup
		var mu sync.Mutex
		var firstErr error

		wg.Add(2)
		go func() {
			defer wg.Done()
			req, err := http.NewRequestWithContext(ctx, http.MethodGet, "https://www.googleapis.com/oauth2/v3/userinfo", nil)
			if err != nil {
				mu.Lock()
				firstErr = err
				mu.Unlock()
				return
			}
			req.Header.Set("Authorization", "Bearer "+token)
			resp, err := c.getHTTPClient().Do(req)
			if err != nil {
				mu.Lock()
				firstErr = err
				mu.Unlock()
				return
			}
			defer resp.Body.Close()
			body, _ := io.ReadAll(resp.Body)
			if resp.StatusCode != http.StatusOK {
				mu.Lock()
				firstErr = fmt.Errorf("userinfo HTTP %d", resp.StatusCode)
				mu.Unlock()
				return
			}
			var ui struct {
				Name      string `json:"name"`
				GivenName string `json:"given_name"`
				Email     string `json:"email"`
				Picture   string `json:"picture"`
			}
			if json.Unmarshal(body, &ui) == nil {
				mu.Lock()
				prof.Name = ui.Name
				prof.GivenName = ui.GivenName
				prof.Email = ui.Email
				prof.Picture = ui.Picture
				mu.Unlock()
			}
		}()

		go func() {
			defer wg.Done()
			if st, err := c.GetUserStatus(ctx); err == nil && st != nil {
				mu.Lock()
				prof.Tier = st.Tier
				mu.Unlock()
			}
		}()

		wg.Wait()
		if prof.Name == "" && prof.Email == "" && firstErr != nil {
			return nil, firstErr
		}
		return prof, nil
	})
}

// FetchAvatarBytes는 허용된 Google 이미지 호스트에서 프로필 사진 바이트와 Content-Type을 안전하게 다운로드한다.
func (c *Client) FetchAvatarBytes(ctx context.Context, pictureURL string) ([]byte, string, error) {
	parsed, err := url.Parse(strings.TrimSpace(pictureURL))
	if err != nil || parsed.Scheme != "https" {
		return nil, "", fmt.Errorf("허용되지 않는 아바타 URL입니다")
	}
	if !isAllowedGoogleImageHost(parsed.Hostname()) {
		return nil, "", fmt.Errorf("허용되지 않는 아바타 호스트입니다: %s", parsed.Hostname())
	}
	req, err := http.NewRequestWithContext(ctx, http.MethodGet, parsed.String(), nil)
	if err != nil {
		return nil, "", err
	}
	resp, err := c.getImageClient().Do(req)
	if err != nil {
		return nil, "", err
	}
	defer resp.Body.Close()
	if resp.StatusCode != http.StatusOK {
		return nil, "", fmt.Errorf("아바타 다운로드 HTTP %d", resp.StatusCode)
	}
	data, err := io.ReadAll(io.LimitReader(resp.Body, 2<<20))
	if err != nil {
		return nil, "", err
	}
	ct := resp.Header.Get("Content-Type")
	if ct == "" {
		ct = "image/jpeg"
	}
	return data, ct, nil
}

// ListConversations는 현재 계정의 대화방 목록을 조회한다.
func (c *Client) ListConversations(ctx context.Context, pageSize int32, pageToken string) (*proto.ListConversationsResult, error) {
	reqBytes := proto.EncodeListConversationsRequest(c.buildChatMetadata(), pageSize, pageToken)
	respBytes, err := c.invokeUnary(ctx, PathListConversations, reqBytes)
	if err != nil {
		return nil, err
	}
	return proto.DecodeListConversationsResponse(respBytes)
}

// ListConversationsWithFilter는 지정한 필터로 대화방 목록을 조회한다.
func (c *Client) ListConversationsWithFilter(ctx context.Context, pageSize int32, pageToken string, filter proto.ListConversationsFilter) (*proto.ListConversationsResult, error) {
	reqBytes := proto.EncodeListConversationsRequestWithFilter(c.buildChatMetadata(), pageSize, pageToken, filter)
	respBytes, err := c.invokeUnary(ctx, PathListConversations, reqBytes)
	if err != nil {
		return nil, err
	}
	return proto.DecodeListConversationsResponse(respBytes)
}

// ListAllConversations는 고정 대화·일반 대화·에이전트(태스크) 대화를 병렬로 조회해 원본 앱과 동일한 순서로 병합한다.
func (c *Client) ListAllConversations(ctx context.Context, pageSize int32) (*proto.ListConversationsResult, error) {
	if pageSize <= 0 {
		pageSize = 50
	}

	var (
		wg        sync.WaitGroup
		pinnedRes *proto.ListConversationsResult
		normalRes *proto.ListConversationsResult
		agentRes  *proto.ListConversationsResult
		normalErr error
	)

	wg.Add(3)
	go func() {
		defer wg.Done()
		pinnedRes, _ = c.ListConversationsWithFilter(ctx, 30, "", proto.ListConversationsFilter{Pinned: true})
	}()
	go func() {
		defer wg.Done()
		normalRes, normalErr = c.ListConversationsWithFilter(ctx, pageSize, "", proto.ListConversationsFilter{IncludeProjects: true})
	}()
	go func() {
		defer wg.Done()
		agentRes, _ = c.ListConversationsWithFilter(ctx, pageSize, "", proto.ListConversationsFilter{IsAgentChat: true})
	}()
	wg.Wait()

	if normalRes == nil && pinnedRes == nil && agentRes == nil {
		return nil, normalErr
	}

	byID := make(map[string]proto.ConversationSummary)
	merge := func(res *proto.ListConversationsResult, forcePinned, forceAgent bool) {
		if res == nil {
			return
		}
		for _, item := range res.Conversations {
			if item.ConversationID == "" || item.Hidden {
				continue
			}
			if forcePinned {
				item.Pinned = true
			}
			if forceAgent {
				item.IsAgentChat = true
			}
			if existing, ok := byID[item.ConversationID]; ok {
				if item.Title != "" && existing.Title == "" {
					existing.Title = item.Title
				}
				existing.Pinned = existing.Pinned || item.Pinned
				existing.IsAgentChat = existing.IsAgentChat || item.IsAgentChat
				existing.HasUnread = existing.HasUnread || item.HasUnread
				if item.TimestampUnix > existing.TimestampUnix {
					existing.TimestampUnix = item.TimestampUnix
				}
				if item.BotID != "" && existing.BotID == "" {
					existing.BotID = item.BotID
				}
				if item.ProjectID != "" && existing.ProjectID == "" {
					existing.ProjectID = item.ProjectID
				}
				byID[item.ConversationID] = existing
			} else {
				byID[item.ConversationID] = item
			}
		}
	}

	merge(pinnedRes, true, false)
	merge(normalRes, false, false)
	merge(agentRes, false, true)

	merged := make([]proto.ConversationSummary, 0, len(byID))
	for _, v := range byID {
		merged = append(merged, v)
	}
	sort.Slice(merged, func(i, j int) bool {
		if merged[i].Pinned != merged[j].Pinned {
			return merged[i].Pinned
		}
		return merged[i].TimestampUnix > merged[j].TimestampUnix
	})

	nextToken := ""
	if normalRes != nil {
		nextToken = normalRes.NextPageToken
	}
	return &proto.ListConversationsResult{
		Conversations: merged,
		NextPageToken: nextToken,
	}, nil
}

// ListBots는 현재 계정의 Gem(커스텀/프리셋 봇) 목록을 조회한다.
func (c *Client) ListBots(ctx context.Context) ([]proto.BotSummary, error) {
	reqBytes := proto.EncodeListBotsRequest(c.buildChatMetadata())
	respBytes, err := c.invokeUnary(ctx, PathListBots, reqBytes)
	if err != nil {
		return nil, err
	}
	return proto.DecodeListBotsResponse(respBytes)
}

// SearchConversations는 서버 측 대화 이력 전문 검색을 수행한다.
func (c *Client) SearchConversations(ctx context.Context, query string, pageSize int32) ([]proto.SearchConversationHit, error) {
	reqBytes := proto.EncodeSearchConversationsRequest(c.buildChatMetadata(), query, pageSize)
	respBytes, err := c.invokeUnary(ctx, PathSearchConversations, reqBytes)
	if err != nil {
		return nil, err
	}
	return proto.DecodeSearchConversationsResponse(respBytes)
}

// ListConversationTurns는 특정 대화방의 질문-응답 턴 이력을 조회한다.
func (c *Client) ListConversationTurns(ctx context.Context, conversationID string, pageSize int32, pageToken string) (*proto.ListConversationTurnsResult, error) {
	reqBytes := proto.EncodeListConversationTurnsRequest(c.buildChatMetadata(), conversationID, pageSize, pageToken)
	respBytes, err := c.invokeUnary(ctx, PathListConversationTurns, reqBytes)
	if err != nil {
		return nil, err
	}
	return proto.DecodeListConversationTurnsResponse(respBytes)
}

// DeleteConversation은 지정한 대화방을 삭제한다.
func (c *Client) DeleteConversation(ctx context.Context, conversationID string) error {
	reqBytes := proto.EncodeDeleteConversationRequest(c.buildChatMetadata(), conversationID)
	_, err := c.invokeUnary(ctx, PathDeleteConversation, reqBytes)
	return err
}

// RenameConversation은 대화방 제목을 변경한다.
func (c *Client) RenameConversation(ctx context.Context, conversationID, title string) error {
	reqBytes := proto.EncodeUpdateConversationTitleRequest(c.buildChatMetadata(), conversationID, title)
	_, err := c.invokeUnary(ctx, PathUpdateConversation, reqBytes)
	return err
}

// PinConversation은 대화방 상단 고정 여부를 변경한다.
func (c *Client) PinConversation(ctx context.Context, conversationID string, pinned bool) error {
	reqBytes := proto.EncodeUpdateConversationPinnedRequest(c.buildChatMetadata(), conversationID, pinned)
	_, err := c.invokeUnary(ctx, PathUpdateConversation, reqBytes)
	return err
}

// AbortGeneration은 진행 중인 응답 생성을 중단한다.
func (c *Client) AbortGeneration(ctx context.Context, conversationID, requestID string) error {
	reqBytes := proto.EncodeAbortGenerationRequest(c.buildChatMetadata(), conversationID, requestID)
	_, err := c.invokeUnary(ctx, PathAbortGeneration, reqBytes)
	return err
}

// StreamQuery는 ProcessQuery gRPC 스트림으로 질문을 전송하고 실시간 청크를 콜백으로 전달한다.
func (c *Client) StreamQuery(ctx context.Context, params proto.ChatQueryParams, onChunk func(proto.ChatStreamChunk) error) (*proto.ChatStreamChunk, error) {
	if strings.TrimSpace(params.Text) == "" && len(params.FilePaths) == 0 && len(params.Attachments) == 0 {
		return nil, fmt.Errorf("질문 내용이 비어 있습니다")
	}
	if params.Locale == "" {
		params.Locale = c.cfg.Locale
	}

	token, err := c.authMgr.AccessToken(ctx)
	if err != nil {
		return nil, err
	}

	if len(params.FilePaths) > 0 {
		uploader := c.getUploader()
		const maxAttachSize = 20 << 20 // 20MB
		validPaths := make([]string, 0, len(params.FilePaths))
		for _, fp := range params.FilePaths {
			fp = strings.TrimSpace(fp)
			if fp == "" {
				continue
			}
			info, statErr := os.Stat(fp)
			if statErr != nil {
				return nil, fmt.Errorf("첨부 파일 확인 실패 (%s): %w", fp, statErr)
			}
			if info.IsDir() {
				return nil, fmt.Errorf("디렉터리는 첨부할 수 없습니다: %s", fp)
			}
			if info.Size() > maxAttachSize {
				return nil, fmt.Errorf("첨부 파일이 20MB를 초과합니다: %s", filepath.Base(fp))
			}
			validPaths = append(validPaths, fp)
		}
		if len(validPaths) > 0 {
			uploaded := make([]proto.ChatAttachment, len(validPaths))
			upCtx, cancelUp := context.WithCancel(ctx)
			defer cancelUp()

			sem := make(chan struct{}, 4)
			var wg sync.WaitGroup
			var errMu sync.Mutex
			var firstUpErr error

			for idx, fp := range validPaths {
				wg.Add(1)
				go func(i int, path string) {
					defer wg.Done()
					select {
					case sem <- struct{}{}:
						defer func() { <-sem }()
					case <-upCtx.Done():
						return
					}
					data, readErr := os.ReadFile(path)
					if readErr != nil {
						errMu.Lock()
						if firstUpErr == nil {
							firstUpErr = fmt.Errorf("첨부 파일 읽기 실패 (%s): %w", path, readErr)
							cancelUp()
						}
						errMu.Unlock()
						return
					}
					blob, upErr := uploader.Upload(upCtx, token, filepath.Base(path), "", data, 1)
					if upErr != nil {
						errMu.Lock()
						if firstUpErr == nil {
							firstUpErr = fmt.Errorf("첨부 파일 업로드 실패 (%s): %w", filepath.Base(path), upErr)
							cancelUp()
						}
						errMu.Unlock()
						return
					}
					uploaded[i] = proto.ChatAttachment{
						BlobID:     blob.BlobID,
						FileType:   blob.FileType,
						MimeType:   blob.MimeType,
						FileSource: blob.FileSource,
						Name:       blob.Name,
					}
				}(idx, fp)
			}
			wg.Wait()
			if firstUpErr != nil {
				return nil, firstUpErr
			}
			params.Attachments = append(params.Attachments, uploaded...)
		}
	}

	if params.AgentTask && strings.TrimSpace(params.TargetDeviceID) != "" {
		if devs, derr := c.ListDevices(ctx); derr == nil {
			wantID := strings.TrimSpace(params.TargetDeviceID)
			for _, dev := range devs {
				if strings.EqualFold(dev.DeviceID, wantID) {
					if params.TargetDeviceName == "" {
						params.TargetDeviceName = dev.DeviceName
					}
					params.TargetDeliveryToken = dev.DeliveryToken
					params.TargetDesktopInfoRaw = dev.RawDesktopInfo
					break
				}
			}
		}
		if params.TargetDeviceName == "" && strings.EqualFold(strings.TrimSpace(params.TargetDeviceID), c.cfg.DeviceID) {
			params.TargetDeviceName = c.cfg.DeviceName
		}
	}

	f1, f2, f3 := proto.EncodeProcessQueryFrames(c.buildChatMetadata(), params)
	var body bytes.Buffer
	body.Write(FrameGRPC(f1))
	body.Write(FrameGRPC(f2))
	body.Write(FrameGRPC(f3))

	grpcURL := "https://" + DefaultHost + PathProcessQuery
	req, err := http.NewRequestWithContext(ctx, http.MethodPost, grpcURL, &body)
	if err != nil {
		return nil, err
	}
	req.Header.Set("Content-Type", "application/grpc")
	req.Header.Set("TE", "trailers")
	req.Header.Set("Authorization", "Bearer "+token)
	req.Header.Set("X-Ios-Bundle-Identifier", BundleID)
	req.Header.Set("Accept-Language", "ko-KR")
	req.Header.Set("User-Agent", "grpc-c++/1.62.0 grpc-objc/1.62.0")

	resp, err := c.h2Client.Do(req)
	if err != nil {
		return nil, fmt.Errorf("ProcessQuery 스트림 연결 실패: %w", err)
	}
	defer resp.Body.Close()

	if resp.StatusCode != http.StatusOK {
		raw, _ := io.ReadAll(resp.Body)
		return nil, fmt.Errorf("ProcessQuery HTTP %d: %s", resp.StatusCode, string(raw))
	}

	final := &proto.ChatStreamChunk{
		Type:           "done",
		ConversationID: params.ConversationID,
	}
	var prevText, prevThoughts string

	for {
		frameBytes, err := ReadGRPCFrame(resp.Body)
		if err == io.EOF {
			grpcStatus := resp.Trailer.Get("Grpc-Status")
			if grpcStatus == "" {
				grpcStatus = resp.Header.Get("Grpc-Status")
			}
			grpcMsg := resp.Trailer.Get("Grpc-Message")
			if grpcMsg == "" {
				grpcMsg = resp.Header.Get("Grpc-Message")
			}
			if grpcStatus != "" && grpcStatus != "0" {
				return final, fmt.Errorf("ProcessQuery gRPC 오류 (status=%s, msg=%s)", grpcStatus, grpcMsg)
			}
			final.TreeCompleted = true
			return final, nil
		}
		if err != nil {
			return final, fmt.Errorf("ProcessQuery 프레임 수신 오류: %w", err)
		}

		chunks, derr := proto.DecodeProcessQueryResponseFrame(frameBytes)
		if derr != nil {
			continue
		}
		for _, ch := range chunks {
			if ch.ConversationID != "" {
				final.ConversationID = ch.ConversationID
			} else if final.ConversationID != "" {
				ch.ConversationID = final.ConversationID
			}
			if ch.RequestID != "" {
				final.RequestID = ch.RequestID
			} else if final.RequestID != "" {
				ch.RequestID = final.RequestID
			}
			if ch.ResponseID != "" {
				final.ResponseID = ch.ResponseID
			} else if final.ResponseID != "" {
				ch.ResponseID = final.ResponseID
			}
			if ch.Title != "" {
				final.Title = ch.Title
			}
			if ch.ModelAttribution != "" {
				final.ModelAttribution = ch.ModelAttribution
			}
			if ch.BardModeID != "" {
				final.BardModeID = ch.BardModeID
			}
			if ch.TargetDeviceName != "" {
				final.TargetDeviceName = ch.TargetDeviceName
			}
			if ch.StatusText != "" {
				final.StatusText = ch.StatusText
			}
			if len(ch.ToolActivities) > 0 {
				final.ToolActivities = ch.ToolActivities
			}

			if ch.Type == "data" {
				if ch.Text != "" {
					if strings.HasPrefix(ch.Text, prevText) {
						ch.Delta = ch.Text[len(prevText):]
					} else if ch.Text != prevText {
						ch.Delta = ch.Text
					}
					prevText = ch.Text
					final.Text = ch.Text
				}
				if ch.Thoughts != "" {
					if strings.HasPrefix(ch.Thoughts, prevThoughts) {
						ch.ThoughtsDelta = ch.Thoughts[len(prevThoughts):]
					} else if ch.Thoughts != prevThoughts {
						ch.ThoughtsDelta = ch.Thoughts
					}
					prevThoughts = ch.Thoughts
					final.Thoughts = ch.Thoughts
				}
				if ch.TreeCompleted {
					final.TreeCompleted = true
				}
			}

			if onChunk != nil && (ch.Type == "data" || ch.Type == "identifiers" || ch.Type == "end_of_data") {
				if cbErr := onChunk(ch); cbErr != nil {
					return final, cbErr
				}
			}
		}
	}
}
