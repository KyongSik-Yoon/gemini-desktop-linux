package proto

import (
	"fmt"
	"html"
	"regexp"
	"strings"
)

var (
	spanTagRe               = regexp.MustCompile(`\[span_\d+\]\((?:start|end)_span\)`)
	followUpTagRe           = regexp.MustCompile(`(?s)<FollowUp\b([^>]*?)/>`)
	elicitationsGroupRe     = regexp.MustCompile(`(?s)<ElicitationsGroup\b([^>]*)>(.*?)</ElicitationsGroup>`)
	elicitationItemRe       = regexp.MustCompile(`(?s)<Elicitation\b([^>]*?)/>`)
	xmlAttrRe               = regexp.MustCompile(`([A-Za-z0-9_:-]+)\s*=\s*"([^"]*)"`)
	incompleteInteractiveRe = regexp.MustCompile(`(?m)(?:^|\n)[ \t]*<ElicitationsGroup(?:\s+[^>]*)?>\s*(?:<[\s\S]*)?$|(?s)<(?:FollowUp|ElicitationsGroup|Elicitation)\b[^>]*$`)
)

// RobinService 대화 클라이언트 열거형 상수
const (
	ClientTypeMacOS    int32 = 21
	UISourceFullScreen int32 = 1
	RobinModeClassic   int32 = 1

	CompletenessUnspecified int32 = 0
	CompletenessPartial     int32 = 1
	CompletenessComplete    int32 = 2

	// 사고 수준(ThinkingLevel) 서버 열거형 상수
	ThinkingLevelUnspecified int32 = 0
	ThinkingLevelLow         int32 = 1 // 낮음 (속도 및 효율성)
	ThinkingLevelHigh        int32 = 2 // 높음 (매우 철저함)
	ThinkingLevelMedium      int32 = 4 // 보통 (균형 잡힌 깊이)

	// 기능 모드(FeatureMode) 열거형 상수
	FeatureModeDeepResearch     int32 = 1
	FeatureModeCanvas           int32 = 2
	FeatureModeCreateFlashCards int32 = 9
	FeatureModeCreateQuiz       int32 = 10
	FeatureModeVideoGeneration  int32 = 11
	FeatureModeImageGeneration  int32 = 14
	FeatureModeMusicGeneration  int32 = 21
	FeatureModeAgent            int32 = 22
	FeatureModeGuidedLearning   int32 = 24
	FeatureModeJanusTasks       int32 = 37
	FeatureModeRemy             int32 = 40
	FeatureModeJanusRemy        int32 = 41
	FeatureModeBlueRemy         int32 = 42
	FeatureModeChatRemy         int32 = 58
)

// ProcessQuery Proto2 확장 필드 번호
const (
	extProcessQueryInitialReq  uint32 = 1000
	extProcessQueryInitialResp uint32 = 1000

	// ProcessQueryInitialRequest 내부 기능 확장
	extClientGenerateFeature uint32 = 1001
	extPrefetchInputFeature  uint32 = 1004
	extHalfCloseFeature      uint32 = 1010

	// ProcessQueryRequest 후속 프레임 확장
	extPrefetchInputReq uint32 = 1007

	// ProcessQueryResponse 스트림 프레임 확장
	extClientGenerateRespStream  uint32 = 1003
	extHalfCloseResp             uint32 = 1009
	extContinuationIdentifiersRs uint32 = 1014
)

// EncodeChatRequestMetadata는 RobinService 대화/스트리밍 RPC용 RequestMetadata를 인코딩한다.
func EncodeChatRequestMetadata(locale, timeZone, deviceID string) []byte {
	if locale == "" {
		locale = "ko-KR"
	}
	if timeZone == "" {
		timeZone = "Asia/Seoul"
	}

	loc := NewBuilder()
	loc.AppendString(1, locale)

	logMeta := NewBuilder()
	if deviceID != "" {
		logMeta.AppendString(17, strings.ToUpper(deviceID))
	}

	tz := NewBuilder()
	tz.AppendString(1, timeZone)

	b := NewBuilder()
	b.AppendMessage(2, loc.Bytes())
	b.AppendMessage(3, logMeta.Bytes())
	b.AppendInt32(4, ClientTypeMacOS)
	b.AppendInt32(5, UISourceFullScreen)
	b.AppendString(7, locale)
	b.AppendMessage(19, tz.Bytes())
	return b.Bytes()
}

// ConversationSummary는 대화 목록의 단일 대화방 요약 정보다.
type ConversationSummary struct {
	ConversationID string `json:"conversation_id"`
	Title          string `json:"title"`
	Pinned         bool   `json:"pinned"`
	Hidden         bool   `json:"hidden,omitempty"`
	IsAgentChat    bool   `json:"is_agent_chat,omitempty"`
	HasUnread      bool   `json:"has_unread,omitempty"`
	TimestampUnix  int64  `json:"timestamp_unix"`
	BotID          string `json:"bot_id,omitempty"`
	ProjectID      string `json:"project_id,omitempty"`
}

// ListConversationsFilter는 대화 목록 조회 시 서버 필터 조건이다.
type ListConversationsFilter struct {
	Pinned          bool
	IncludeHidden   bool
	BotID           string
	IncludeProjects bool
	IsAgentChat     bool
}

// ListConversationsResult는 ListConversations 응답 결과다.
type ListConversationsResult struct {
	Conversations []ConversationSummary `json:"conversations"`
	NextPageToken string                `json:"next_page_token,omitempty"`
}

// EncodeListConversationsRequest는 대화 목록 조회 요청을 인코딩한다.
func EncodeListConversationsRequest(metadata []byte, pageSize int32, pageToken string) []byte {
	return EncodeListConversationsRequestWithFilter(metadata, pageSize, pageToken, ListConversationsFilter{})
}

// EncodeListConversationsRequestWithFilter는 필터 조건을 포함한 대화 목록 조회 요청을 인코딩한다.
func EncodeListConversationsRequestWithFilter(metadata []byte, pageSize int32, pageToken string, filter ListConversationsFilter) []byte {
	b := NewBuilder()
	if pageSize <= 0 {
		pageSize = 30
	}
	b.AppendInt32(1, pageSize)
	b.AppendString(2, pageToken)

	if filter.Pinned || filter.IncludeHidden || filter.BotID != "" || filter.IncludeProjects || filter.IsAgentChat {
		fb := NewBuilder()
		fb.AppendBool(1, filter.Pinned)
		fb.AppendBool(2, filter.IncludeHidden)
		fb.AppendString(3, filter.BotID)
		fb.AppendBool(4, filter.IncludeProjects)
		fb.AppendBool(5, filter.IsAgentChat)
		b.AppendMessage(3, fb.Bytes())
	}

	b.AppendMessage(4, metadata)
	return b.Bytes()
}

// DecodeListConversationsResponse는 대화 목록 응답을 디코딩한다.
func DecodeListConversationsResponse(data []byte) (*ListConversationsResult, error) {
	fields, err := ParseFields(data)
	if err != nil {
		return nil, err
	}
	res := &ListConversationsResult{
		Conversations: []ConversationSummary{},
	}
	for _, f := range fields {
		switch f.Num {
		case 1:
			if f.WireType != WireBytes {
				continue
			}
			conv, err := decodeConversation(f.Bytes)
			if err == nil && conv.ConversationID != "" {
				res.Conversations = append(res.Conversations, conv)
			}
		case 2:
			if f.WireType == WireBytes {
				res.NextPageToken = string(f.Bytes)
			}
		}
	}
	return res, nil
}

func decodeConversation(data []byte) (ConversationSummary, error) {
	fields, err := ParseFields(data)
	if err != nil {
		return ConversationSummary{}, err
	}
	var c ConversationSummary
	for _, f := range fields {
		switch f.Num {
		case 1:
			c.ConversationID = string(f.Bytes)
		case 2:
			c.Title = string(f.Bytes)
		case 3:
			c.Pinned = f.Varint != 0
		case 4:
			c.Hidden = f.Varint != 0
		case 6:
			c.TimestampUnix = decodeTimestampSeconds(f.Bytes)
		case 7: // read/activity state: f1==1인 경우 미확인/새 업데이트
			if f.WireType == WireBytes {
				sf, _ := ParseFields(f.Bytes)
				for _, s := range sf {
					if s.Num == 1 && s.Varint == 1 {
						c.HasUnread = true
					}
				}
			}
		case 8: // notification/unread badge
			if f.WireType == WireBytes {
				sf, _ := ParseFields(f.Bytes)
				for _, s := range sf {
					if s.Num == 2 && s.Varint != 0 {
						c.HasUnread = true
					}
				}
			}
		case 9:
			c.BotID = string(f.Bytes)
		case 15:
			if f.Varint != 0 {
				c.IsAgentChat = true
			}
		case 21:
			c.ProjectID = string(f.Bytes)
		}
	}
	return c, nil
}

func decodeTimestampSeconds(data []byte) int64 {
	fields, err := ParseFields(data)
	if err != nil {
		return 0
	}
	for _, f := range fields {
		if f.Num == 1 && f.WireType == WireVarint {
			return int64(f.Varint)
		}
	}
	return 0
}

// TurnAttachment는 과거 턴 사용자 질문에 첨부된 파일/이미지 메타데이터다.
type TurnAttachment struct {
	Name     string `json:"name"`
	URL      string `json:"url,omitempty"`
	MimeType string `json:"mime_type,omitempty"`
	Width    int32  `json:"width,omitempty"`
	Height   int32  `json:"height,omitempty"`
}

// GeneratedImage는 모델 응답에서 생성된 이미지 메타데이터다.
type GeneratedImage struct {
	URL           string `json:"url"`
	Title         string `json:"title,omitempty"`
	PlaceholderID string `json:"placeholder_id,omitempty"`
}

// TurnArtifact는 모델 응답에 포함된 Canvas/인터랙티브 코드 아티팩트다.
type TurnArtifact struct {
	ID       string `json:"id,omitempty"`
	Title    string `json:"title"`
	Language string `json:"language,omitempty"`
	Content  string `json:"content,omitempty"`
	URL      string `json:"url,omitempty"`
}

// FollowUpPrompt는 모델 응답 하단의 단일 확인형 후속 제안(<FollowUp>)이다.
type FollowUpPrompt struct {
	Label string `json:"label"`
	Query string `json:"query"`
}

// ElicitationItem은 후속 탐색 제안 카드(<Elicitation>) 단일 항목이다.
type ElicitationItem struct {
	Label string `json:"label"`
	Query string `json:"query"`
}

// ElicitationsGroup은 후속 탐색 제안 카드 그룹(<ElicitationsGroup>)이다.
type ElicitationsGroup struct {
	Message string            `json:"message,omitempty"`
	Items   []ElicitationItem `json:"items"`
}

// ConversationTurn은 대화방 내 단일 질문-응답 턴이다.
type ConversationTurn struct {
	ConversationID   string               `json:"conversation_id"`
	RequestID        string               `json:"request_id"`
	ResponseID       string               `json:"response_id"`
	ParentRequestID  string               `json:"parent_request_id,omitempty"`
	ParentResponseID string               `json:"parent_response_id,omitempty"`
	UserText         string               `json:"user_text"`
	UserAttachments  []TurnAttachment     `json:"user_attachments,omitempty"`
	ModelText        string               `json:"model_text"`
	Thoughts         string               `json:"thoughts,omitempty"`
	StatusText       string               `json:"status_text,omitempty"`
	ToolActivities   []RemoteToolActivity `json:"tool_activities,omitempty"`
	GeneratedImages  []GeneratedImage     `json:"generated_images,omitempty"`
	Artifacts        []TurnArtifact       `json:"artifacts,omitempty"`
	FollowUp         *FollowUpPrompt      `json:"follow_up,omitempty"`
	Elicitations     *ElicitationsGroup   `json:"elicitations,omitempty"`
	IsScheduledRun   bool                 `json:"is_scheduled_run,omitempty"`
	ScheduledTitle   string               `json:"scheduled_title,omitempty"`
	ModelAttribution  string               `json:"model_attribution,omitempty"`
	BardModeID        string               `json:"bard_mode_id,omitempty"`
	CompletenessState int32                `json:"completeness_state,omitempty"`
	Aborted           bool                 `json:"aborted,omitempty"`
	TimestampUnix     int64                `json:"timestamp_unix"`
}

// ListConversationTurnsResult는 ListConversationTurns 응답 결과다.
type ListConversationTurnsResult struct {
	Turns         []ConversationTurn `json:"turns"`
	NextPageToken string             `json:"next_page_token,omitempty"`
}

// EncodeListConversationTurnsRequest는 대화 턴 조회 요청을 인코딩한다.
func EncodeListConversationTurnsRequest(metadata []byte, conversationID string, pageSize int32, pageToken string) []byte {
	b := NewBuilder()
	b.AppendString(1, conversationID)
	if pageSize <= 0 {
		pageSize = 50
	}
	b.AppendInt32(2, pageSize)
	b.AppendString(3, pageToken)
	b.AppendMessage(6, metadata)
	return b.Bytes()
}

// DecodeListConversationTurnsResponse는 대화 턴 조회 응답을 디코딩한다.
func DecodeListConversationTurnsResponse(data []byte) (*ListConversationTurnsResult, error) {
	fields, err := ParseFields(data)
	if err != nil {
		return nil, err
	}
	res := &ListConversationTurnsResult{
		Turns: []ConversationTurn{},
	}
	for _, f := range fields {
		switch f.Num {
		case 1:
			if f.WireType != WireBytes {
				continue
			}
			turn, err := decodeConversationTurn(f.Bytes)
			if err == nil {
				res.Turns = append(res.Turns, turn)
			}
		case 2:
			if f.WireType == WireBytes {
				res.NextPageToken = string(f.Bytes)
			}
		}
	}
	return res, nil
}

func decodeConversationTurn(data []byte) (ConversationTurn, error) {
	fields, err := ParseFields(data)
	if err != nil {
		return ConversationTurn{}, err
	}
	var t ConversationTurn
	for _, f := range fields {
		switch f.Num {
		case 1: // current_turn_identifiers (ResponseIdentifiers)
			t.ConversationID, t.RequestID = decodeResponseIdentifiers(f.Bytes)
		case 2: // parent_turn_identifiers (RequestIdentifiers)
			_, t.ParentRequestID, t.ParentResponseID = decodeRequestIdentifiers(f.Bytes)
		case 3: // SavedGenerateRequest
			rf, _ := ParseFields(f.Bytes)
			for _, r := range rf {
				if r.Num == 1 && r.WireType == WireBytes { // SavedUserQuery
					qf, _ := ParseFields(r.Bytes)
					for _, q := range qf {
						switch q.Num {
						case 1:
							if q.WireType == WireBytes {
								t.UserText = string(q.Bytes)
							}
						case 5: // 사용자 첨부 파일/이미지 목록
							if q.WireType == WireBytes {
								t.UserAttachments = append(t.UserAttachments, decodeSavedUserAttachments(q.Bytes)...)
							}
						}
					}
				}
			}
		case 4: // SavedGenerateResponse
			var selectedID string
			var candidates []decodedModelResponse
			sf, _ := ParseFields(f.Bytes)
			for _, s := range sf {
				switch s.Num {
				case 1:
					if s.WireType == WireBytes {
						candidates = append(candidates, decodeModelResponse(s.Bytes))
					}
				case 4:
					selectedID = string(s.Bytes)
				case 5: // repeated StatusUpdateResponse (히스토리 사고/도구 요약)
					if s.WireType == WireBytes {
						st, goalTitle, acts := decodeStatusUpdateResponseDetail(s.Bytes)
						if st != "" {
							t.StatusText = st
						}
						if goalTitle != "" && t.ScheduledTitle == "" {
							t.ScheduledTitle = goalTitle
						}
						if len(acts) > 0 {
							t.ToolActivities = append(t.ToolActivities, acts...)
						}
					}
				case 11:
					t.Aborted = s.Varint != 0
				case 17:
					t.ModelAttribution = string(s.Bytes)
				case 18:
					t.BardModeID = string(s.Bytes)
				}
			}
			chosen := pickModelResponse(candidates, selectedID)
			t.ResponseID = chosen.ResponseID
			t.ModelText = chosen.Text
			t.Thoughts = chosen.Thoughts
			t.CompletenessState = chosen.CompletenessState
			if len(chosen.Artifacts) > 0 {
				t.Artifacts = chosen.Artifacts
			}
			t.FollowUp = chosen.FollowUp
			t.Elicitations = chosen.Elicitations
		case 5: // Timestamp
			t.TimestampUnix = decodeTimestampSeconds(f.Bytes)
		case 17: // 예약 실행 메타데이터 (f1 == 1)
			if f.WireType == WireBytes {
				sf, _ := ParseFields(f.Bytes)
				for _, s := range sf {
					if s.Num == 1 && s.WireType == WireVarint && s.Varint != 0 {
						t.IsScheduledRun = true
					}
				}
			}
		case 18: // request_timestamp_micros (Timestamp가 비었을 때 보조)
			if t.TimestampUnix == 0 && f.WireType == WireVarint {
				t.TimestampUnix = int64(f.Varint) / 1_000_000
			}
		}
	}
	if len(t.ToolActivities) > 1 {
		t.ToolActivities = dedupToolActivities(t.ToolActivities)
	}
	return t, nil
}

func decodeSavedUserAttachments(data []byte) []TurnAttachment {
	fields, err := ParseFields(data)
	if err != nil {
		return nil
	}
	var out []TurnAttachment
	for _, f := range fields {
		if f.WireType != WireBytes {
			continue
		}
		switch f.Num {
		case 4: // Scotty 업로드 블롭 첨부
			af, err := ParseFields(f.Bytes)
			if err != nil {
				continue
			}
			var att TurnAttachment
			for _, a := range af {
				switch a.Num {
				case 3:
					if a.WireType == WireBytes {
						att.Name = strings.TrimSpace(string(a.Bytes))
					}
				case 4:
					if a.WireType == WireBytes {
						att.URL = strings.TrimSpace(string(a.Bytes))
					}
				case 12:
					if a.WireType == WireBytes {
						att.MimeType = strings.TrimSpace(string(a.Bytes))
					}
				case 16:
					if a.WireType == WireBytes {
						df, _ := ParseFields(a.Bytes)
						for _, d := range df {
							if d.Num == 1 && d.WireType == WireVarint {
								att.Width = int32(d.Varint)
							} else if d.Num == 2 && d.WireType == WireVarint {
								att.Height = int32(d.Varint)
							}
						}
					}
				}
			}
			if att.Name != "" || att.URL != "" {
				out = append(out, att)
			}
		case 5: // Google Drive 파일 첨부 (f1=resource_id, f2=name, f3=url/mime)
			df, err := ParseFields(f.Bytes)
			if err != nil {
				continue
			}
			var att TurnAttachment
			var resourceID string
			for _, d := range df {
				if d.WireType != WireBytes {
					continue
				}
				switch d.Num {
				case 1:
					resourceID = strings.TrimSpace(string(d.Bytes))
				case 2:
					att.Name = strings.TrimSpace(string(d.Bytes))
				case 3:
					v := strings.TrimSpace(string(d.Bytes))
					if strings.HasPrefix(v, "http://") || strings.HasPrefix(v, "https://") {
						att.URL = v
					} else if att.MimeType == "" {
						att.MimeType = v
					}
				}
			}
			if att.Name == "" && resourceID != "" {
				att.Name = "Drive: " + resourceID
			}
			if att.URL == "" && resourceID != "" {
				att.URL = "https://drive.google.com/file/d/" + resourceID + "/view"
			}
			if att.Name != "" || att.URL != "" {
				out = append(out, att)
			}
		}
	}
	return out
}

type decodedModelResponse struct {
	ResponseID        string
	Text              string
	Thoughts          string
	Artifacts         []TurnArtifact
	FollowUp          *FollowUpPrompt
	Elicitations      *ElicitationsGroup
	CompletenessState int32
}

func parseXMLAttrs(raw string) map[string]string {
	matches := xmlAttrRe.FindAllStringSubmatch(raw, -1)
	if len(matches) == 0 {
		return nil
	}
	out := make(map[string]string, len(matches))
	for _, m := range matches {
		out[m[1]] = strings.TrimSpace(html.UnescapeString(m[2]))
	}
	return out
}

// markdownCodeRanges는 마크다운 펜스 코드 블록(```/~~~)과 인라인 코드 스팬(`...`)의
// 바이트 범위 [start, end) 목록을 반환한다.
func markdownCodeRanges(s string) [][2]int {
	var ranges [][2]int
	n := len(s)
	i := 0
	lineStart := true

	for i < n {
		if lineStart {
			// 들여쓰기 공백(최대 3칸) 뒤 펜스 코드 블록 시작 확인
			ws := i
			for ws < n && ws-i < 3 && s[ws] == ' ' {
				ws++
			}
			if ws+2 < n && (s[ws] == '`' || s[ws] == '~') {
				ch := s[ws]
				j := ws
				for j < n && s[j] == ch {
					j++
				}
				fenceLen := j - ws
				if fenceLen >= 3 {
					// 여는 펜스 줄 끝까지 이동 (백틱 펜스의 info string에 백틱이 있으면 인라인 코드로 간주)
					nl := strings.IndexByte(s[j:], '\n')
					infoEnd := n
					if nl >= 0 {
						infoEnd = j + nl
					}
					if ch != '`' || !strings.ContainsRune(s[j:infoEnd], '`') {
						blockStart := i
						pos := infoEnd
						if pos < n {
							pos++
						}
						for pos < n {
							nextNL := strings.IndexByte(s[pos:], '\n')
							lineEnd := n
							if nextNL >= 0 {
								lineEnd = pos + nextNL
							}
							cws := pos
							for cws < lineEnd && cws-pos < 3 && s[cws] == ' ' {
								cws++
							}
							cj := cws
							for cj < lineEnd && s[cj] == ch {
								cj++
							}
							if cj-cws >= fenceLen && strings.TrimSpace(s[cj:lineEnd]) == "" {
								pos = lineEnd
								if pos < n {
									pos++
								}
								break
							}
							pos = lineEnd
							if pos < n {
								pos++
							}
						}
						ranges = append(ranges, [2]int{blockStart, pos})
						i = pos
						lineStart = true
						continue
					}
				}
			}
		}

		if s[i] == '`' {
			j := i
			for j < n && s[j] == '`' {
				j++
			}
			tickLen := j - i
			k := j
			found := -1
			for k < n {
				if s[k] == '`' {
					m := k
					for m < n && s[m] == '`' {
						m++
					}
					if m-k == tickLen {
						found = m
						break
					}
					k = m
					continue
				}
				k++
			}
			if found > 0 {
				ranges = append(ranges, [2]int{i, found})
				i = found
				lineStart = false
				continue
			}
			i = j
			lineStart = false
			continue
		}

		lineStart = s[i] == '\n'
		i++
	}
	return ranges
}

func overlapsAnyRange(start, end int, ranges [][2]int) bool {
	for _, r := range ranges {
		if start < r[1] && end > r[0] {
			return true
		}
		if r[0] >= end {
			break
		}
	}
	return false
}

func replaceOutsideCode(s string, re *regexp.Regexp, fn func(match string, submatches []string) string) string {
	idxs := re.FindAllStringSubmatchIndex(s, -1)
	if len(idxs) == 0 {
		return s
	}
	codeRanges := markdownCodeRanges(s)
	var out strings.Builder
	last := 0
	replaced := false
	for _, idx := range idxs {
		start, end := idx[0], idx[1]
		if overlapsAnyRange(start, end, codeRanges) {
			continue
		}
		subs := make([]string, len(idx)/2)
		for g := 0; g < len(idx); g += 2 {
			if idx[g] >= 0 && idx[g+1] >= 0 {
				subs[g/2] = s[idx[g]:idx[g+1]]
			}
		}
		out.WriteString(s[last:start])
		out.WriteString(fn(s[start:end], subs))
		last = end
		replaced = true
	}
	if !replaced {
		return s
	}
	out.WriteString(s[last:])
	return out.String()
}

// extractInteractiveSuggestions는 DisplayText(f2)에 포함된 <FollowUp> 및 <ElicitationsGroup>
// 태그를 구조화된 후속 제안으로 분리하고 본문에서 해당 XML 블록과 미완성 스트리밍 꼬리를 제거한다.
func extractInteractiveSuggestions(raw string) (string, *FollowUpPrompt, *ElicitationsGroup, bool) {
	return extractInteractiveSuggestionsWithMode(raw, true)
}

func extractInteractiveSuggestionsWithMode(raw string, allowIncompleteTail bool) (string, *FollowUpPrompt, *ElicitationsGroup, bool) {
	if !strings.Contains(raw, "<FollowUp") &&
		!strings.Contains(raw, "<ElicitationsGroup") &&
		!strings.Contains(raw, "<Elicitation") {
		return raw, nil, nil, false
	}

	var followUp *FollowUpPrompt
	var elicitations *ElicitationsGroup
	hadTag := false

	cleaned := replaceOutsideCode(raw, followUpTagRe, func(_ string, sub []string) string {
		hadTag = true
		if len(sub) >= 2 && followUp == nil {
			attrs := parseXMLAttrs(sub[1])
			label := attrs["label"]
			query := attrs["query"]
			if query == "" {
				query = label
			}
			if label != "" {
				followUp = &FollowUpPrompt{Label: label, Query: query}
			}
		}
		return ""
	})

	cleaned = replaceOutsideCode(cleaned, elicitationsGroupRe, func(_ string, sub []string) string {
		hadTag = true
		if len(sub) >= 3 {
			attrs := parseXMLAttrs(sub[1])
			msg := attrs["message"]
			var items []ElicitationItem
			for _, im := range elicitationItemRe.FindAllStringSubmatch(sub[2], -1) {
				if len(im) < 2 {
					continue
				}
				ia := parseXMLAttrs(im[1])
				label := ia["label"]
				query := ia["query"]
				if query == "" {
					query = label
				}
				if label != "" {
					items = append(items, ElicitationItem{Label: label, Query: query})
				}
			}
			if len(items) > 0 && elicitations == nil {
				elicitations = &ElicitationsGroup{Message: msg, Items: items}
			}
		}
		return ""
	})

	// 스트리밍 도중(미완성 응답) 코드 영역 바깥 꼬리에 닫히지 않은 XML 태그가 노출되지 않도록 마스킹한다.
	if allowIncompleteTail {
		if loc := incompleteInteractiveRe.FindStringIndex(cleaned); loc != nil {
			codeRanges := markdownCodeRanges(cleaned)
			if !overlapsAnyRange(loc[0], loc[1], codeRanges) {
				hadTag = true
				cleaned = cleaned[:loc[0]]
			}
		}
	}

	if hadTag {
		cleaned = strings.TrimRight(cleaned, " \t\r\n")
	}
	return cleaned, followUp, elicitations, hadTag
}

// stripFlattenedSuggestions는 서버 정제 텍스트(f4) 꼬리에 평문화되어 붙은
// FollowUp/ElicitationsGroup 라벨을 제거해 본문과 카드 중복 표시를 방지한다.
func stripFlattenedSuggestions(clean string, followUp *FollowUpPrompt, elicitations *ElicitationsGroup) (string, bool) {
	trimmed := strings.TrimRight(clean, " \t\r\n")
	strippedAny := false

	if elicitations != nil && len(elicitations.Items) > 0 {
		var lines []string
		if elicitations.Message != "" {
			lines = append(lines, elicitations.Message)
		}
		for _, item := range elicitations.Items {
			lines = append(lines, item.Label)
		}
		suffix := strings.Join(lines, "\n")
		if strings.HasSuffix(trimmed, suffix) {
			trimmed = strings.TrimRight(strings.TrimSuffix(trimmed, suffix), " \t\r\n")
			strippedAny = true
		}
	}

	if followUp != nil && followUp.Label != "" {
		if strings.HasSuffix(trimmed, followUp.Label) {
			trimmed = strings.TrimRight(strings.TrimSuffix(trimmed, followUp.Label), " \t\r\n")
			strippedAny = true
		}
	}

	return trimmed, strippedAny
}

func decodeModelResponse(data []byte) decodedModelResponse {
	var out decodedModelResponse
	var displayText, cleanText string
	fields, _ := ParseFields(data)
	for _, f := range fields {
		switch f.Num {
		case 1:
			out.ResponseID = string(f.Bytes)
		case 2: // DisplayText
			displayText = decodeDisplayText(f.Bytes)
		case 4: // 인용 마크업이 제거된 정제 마크다운 텍스트
			if f.WireType == WireBytes && len(f.Bytes) > 0 {
				cleanText = string(f.Bytes)
			}
		case 12: // Thoughts
			tfs, _ := ParseFields(f.Bytes)
			for _, tf := range tfs {
				if tf.Num == 1 && tf.WireType == WireBytes {
					out.Thoughts = decodeDisplayText(tf.Bytes)
				}
			}
		case 13: // Canvas / Immersive 아티팩트
			if f.WireType == WireBytes {
				if art, ok := decodeTurnArtifact(f.Bytes); ok {
					out.Artifacts = append(out.Artifacts, art)
				}
			}
		case 19:
			out.CompletenessState = int32(f.Varint)
		}
	}

	allowIncompleteTail := out.CompletenessState != CompletenessComplete
	strippedDisplay, followUp, elicitations, hadInteractive := extractInteractiveSuggestionsWithMode(displayText, allowIncompleteTail)
	out.FollowUp = followUp
	out.Elicitations = elicitations

	if hadInteractive {
		if cleanText != "" {
			if cleaned, ok := stripFlattenedSuggestions(cleanText, followUp, elicitations); ok {
				out.Text = cleaned
			} else {
				out.Text = strings.TrimRight(spanTagRe.ReplaceAllString(strippedDisplay, ""), " \t\r\n")
			}
		} else {
			out.Text = strings.TrimRight(spanTagRe.ReplaceAllString(strippedDisplay, ""), " \t\r\n")
		}
	} else if cleanText != "" {
		out.Text = cleanText
	} else {
		out.Text = spanTagRe.ReplaceAllString(displayText, "")
	}
	return out
}

func decodeTurnArtifact(data []byte) (TurnArtifact, bool) {
	fields, err := ParseFields(data)
	if err != nil {
		return TurnArtifact{}, false
	}
	var art TurnArtifact
	var fencedFallback string
	for _, f := range fields {
		if f.WireType != WireBytes {
			continue
		}
		switch f.Num {
		case 1: // 메타데이터 (f1=ID/파일명, f13=언어)
			mf, _ := ParseFields(f.Bytes)
			for _, m := range mf {
				if m.WireType != WireBytes {
					continue
				}
				switch m.Num {
				case 1:
					art.ID = strings.TrimSpace(string(m.Bytes))
				case 13:
					if art.Language == "" {
						art.Language = strings.TrimSpace(string(m.Bytes))
					}
				}
			}
		case 3: // 본문 컨테이너 (f1.f5.{f1: code, f2: lang})
			f1s, _ := ParseFields(f.Bytes)
			for _, f1 := range f1s {
				if f1.Num != 1 || f1.WireType != WireBytes {
					continue
				}
				f5s, _ := ParseFields(f1.Bytes)
				for _, f5 := range f5s {
					if f5.Num != 5 || f5.WireType != WireBytes {
						continue
					}
					cfs, _ := ParseFields(f5.Bytes)
					for _, cf := range cfs {
						if cf.WireType != WireBytes {
							continue
						}
						switch cf.Num {
						case 1:
							if len(cf.Bytes) > 0 {
								art.Content = string(cf.Bytes)
							}
						case 2:
							if len(cf.Bytes) > 0 {
								art.Language = strings.TrimSpace(string(cf.Bytes))
							}
						}
					}
				}
			}
		case 4: // 코드 펜스 포함 본문 폴백
			if len(f.Bytes) > 0 {
				fencedFallback = string(f.Bytes)
			}
		case 6: // 제목
			art.Title = strings.TrimSpace(string(f.Bytes))
		case 7: // 웹 Canvas URL
			art.URL = strings.TrimSpace(string(f.Bytes))
		}
	}
	if art.Content == "" && fencedFallback != "" {
		art.Content = stripOuterCodeFence(fencedFallback)
	}
	if art.Title == "" && art.ID != "" {
		art.Title = art.ID
	}
	if art.Title == "" && art.Content == "" && art.URL == "" {
		return TurnArtifact{}, false
	}
	return art, true
}

func stripOuterCodeFence(s string) string {
	trimmed := strings.TrimSpace(s)
	if !strings.HasPrefix(trimmed, "```") {
		return s
	}
	idx := strings.IndexByte(trimmed, '\n')
	if idx < 0 {
		return s
	}
	body := trimmed[idx+1:]
	if strings.HasSuffix(body, "```") {
		body = strings.TrimSuffix(body, "```")
		body = strings.TrimSuffix(body, "\n")
	}
	return body
}

func pickModelResponse(candidates []decodedModelResponse, selectedID string) decodedModelResponse {
	if len(candidates) == 0 {
		return decodedModelResponse{}
	}
	if selectedID != "" {
		for _, c := range candidates {
			if c.ResponseID == selectedID {
				return c
			}
		}
	}
	return candidates[0]
}

func decodeDisplayText(data []byte) string {
	fields, _ := ParseFields(data)
	for _, f := range fields {
		if f.Num == 1 && f.WireType == WireBytes {
			return string(f.Bytes)
		}
	}
	return ""
}

func decodeResponseIdentifiers(data []byte) (convID, reqID string) {
	fields, _ := ParseFields(data)
	for _, f := range fields {
		switch f.Num {
		case 1:
			convID = string(f.Bytes)
		case 2:
			reqID = string(f.Bytes)
		}
	}
	return convID, reqID
}

func decodeRequestIdentifiers(data []byte) (convID, parentReqID, parentRespID string) {
	fields, _ := ParseFields(data)
	for _, f := range fields {
		switch f.Num {
		case 1:
			convID = string(f.Bytes)
		case 2:
			parentReqID = string(f.Bytes)
		case 3:
			parentRespID = string(f.Bytes)
		}
	}
	return convID, parentReqID, parentRespID
}

// ChatAttachment는 사용자 질문에 첨부할 Scotty 업로드 블롭 또는 Google Drive 파일 정보다.
type ChatAttachment struct {
	BlobID          string `json:"blob_id,omitempty"`
	FileType        int32  `json:"file_type,omitempty"`
	MimeType        string `json:"mime_type,omitempty"`
	FileSource      int32  `json:"file_source,omitempty"`
	Name            string `json:"name,omitempty"`
	DriveResourceID string `json:"drive_resource_id,omitempty"`
}

// ChatQueryParams는 ProcessQuery 스트리밍 질문 전송 파라미터다.
type ChatQueryParams struct {
	Text                 string           `json:"text"`
	ConversationID       string           `json:"conversation_id,omitempty"`
	ParentRequestID      string           `json:"parent_request_id,omitempty"`
	ParentResponseID     string           `json:"parent_response_id,omitempty"`
	Temporary            bool             `json:"temporary,omitempty"`
	BotID                string           `json:"bot_id,omitempty"`
	ProjectID            string           `json:"project_id,omitempty"`
	ModeCategory         int32            `json:"mode_category,omitempty"`
	ThinkingLevel        int32            `json:"thinking_level,omitempty"`
	ModeVariantID        string           `json:"mode_variant_id,omitempty"`
	SkipThinking         bool             `json:"skip_thinking,omitempty"`
	AgentTask            bool             `json:"agent_task,omitempty"`
	FeatureMode          int32            `json:"feature_mode,omitempty"`
	PersonalIntelligence bool             `json:"personal_intelligence,omitempty"`
	TargetDeviceID       string           `json:"target_device_id,omitempty"`
	TargetDeviceName     string           `json:"target_device_name,omitempty"`
	TargetDeliveryToken  string           `json:"-"`
	TargetDesktopInfoRaw []byte           `json:"-"`
	TargetIsLocalSession bool             `json:"target_is_local_session,omitempty"`
	Locale               string           `json:"locale,omitempty"`
	FilePaths            []string         `json:"file_paths,omitempty"`
	Attachments          []ChatAttachment `json:"attachments,omitempty"`
}

// EncodeProcessQueryFrames는 ProcessQuery 3프레임(initial, prefetch, commit) 페이로드를 생성한다.
func EncodeProcessQueryFrames(metadata []byte, p ChatQueryParams) (frame1, frame2, frame3 []byte) {
	locale := p.Locale
	if locale == "" {
		locale = "ko-KR"
	}

	uq := NewBuilder()
	uq.AppendString(1, p.Text)
	for _, att := range p.Attachments {
		if att.DriveResourceID != "" {
			drv := NewBuilder()
			drv.AppendString(1, att.DriveResourceID)
			uqa := NewBuilder()
			uqa.AppendString(2, att.Name)
			uqa.AppendMessage(5, drv.Bytes()) // DriveFile
			uq.AppendMessage(3, uqa.Bytes())
			continue
		}
		if att.BlobID == "" {
			continue
		}
		blob := NewBuilder()
		blob.AppendString(1, att.BlobID)
		blob.AppendInt32(2, att.FileType)
		blob.AppendString(3, att.MimeType)
		blob.AppendInt32(4, att.FileSource)

		uqa := NewBuilder()
		uqa.AppendMessage(1, blob.Bytes())
		uqa.AppendString(2, att.Name)
		uq.AppendMessage(3, uqa.Bytes())
	}

	loc := NewBuilder()
	loc.AppendString(1, locale)

	genReq := NewBuilder()
	genReq.AppendMessage(1, uq.Bytes())
	genReq.AppendMessage(2, loc.Bytes())

	if p.ConversationID != "" || p.ParentRequestID != "" || p.ParentResponseID != "" {
		ids := NewBuilder()
		ids.AppendString(1, p.ConversationID)
		ids.AppendString(2, p.ParentRequestID)
		ids.AppendString(3, p.ParentResponseID)
		genReq.AppendMessage(4, ids.Bytes())
	}

	genReq.AppendBool(5, true) // enable_token_streaming
	genReq.AppendMessage(13, metadata)
	genReq.AppendInt32(18, RobinModeClassic)
	genReq.AppendString(20, p.BotID)
	featureMode := p.FeatureMode
	if featureMode == 0 && p.AgentTask {
		featureMode = FeatureModeJanusRemy
	}
	if featureMode != 0 {
		genReq.AppendInt32(26, featureMode) // user_selected_feature_mode
	}
	genReq.AppendBool(31, p.Temporary)
	genReq.AppendBool(38, p.SkipThinking)
	genReq.AppendInt32(47, p.ModeCategory)
	genReq.AppendInt32(48, p.ThinkingLevel)
	if p.AgentTask {
		genReq.AppendBool(49, true) // is_agent_chat
	}
	if strings.TrimSpace(p.TargetDeviceID) != "" {
		rdt := NewBuilder()
		rdt.AppendString(1, strings.ToUpper(strings.TrimSpace(p.TargetDeviceID)))
		rdt.AppendString(2, p.TargetDeviceName)
		if len(p.TargetDesktopInfoRaw) > 0 {
			rdt.AppendMessage(3, p.TargetDesktopInfoRaw)
		}
		rdt.AppendString(4, p.TargetDeliveryToken)
		if p.TargetIsLocalSession {
			rdt.AppendBool(5, true)
		}
		rdt.AppendInt32(7, RemoteDeviceTypeMacOS)
		genReq.AppendMessage(51, rdt.Bytes()) // remote_device_target
	}
	genReq.AppendString(66, p.ModeVariantID)
	genReq.AppendString(70, p.ProjectID)

	genFeat := NewBuilder()
	genFeat.AppendMessage(1, genReq.Bytes()) // template

	initReq := NewBuilder()
	initReq.AppendMessage(1, metadata)
	initReq.AppendMessage(extClientGenerateFeature, genFeat.Bytes())
	initReq.AppendMessage(extPrefetchInputFeature, nil)
	initReq.AppendMessage(extHalfCloseFeature, nil)

	f1 := NewBuilder()
	f1.AppendInt32(1, 1) // turn_sequence_number
	f1.AppendMessage(extProcessQueryInitialReq, initReq.Bytes())

	prefetch := NewBuilder()
	prefetch.AppendString(1, p.Text) // query
	f2 := NewBuilder()
	f2.AppendInt32(1, 1)
	f2.AppendMessage(extPrefetchInputReq, prefetch.Bytes())

	commit := NewBuilder()
	commit.AppendMessage(2, nil) // commit = google.protobuf.Empty
	f3 := NewBuilder()
	f3.AppendInt32(1, 1)
	f3.AppendMessage(extPrefetchInputReq, commit.Bytes())

	return f1.Bytes(), f2.Bytes(), f3.Bytes()
}

// RemoteToolActivity는 에이전트 태스크 실행 중 서버가 보고하는 도구 호출 진행 상태다.
type RemoteToolActivity struct {
	ID          string `json:"id,omitempty"`
	Name        string `json:"name"`
	Action      string `json:"action,omitempty"`
	Summary     string `json:"summary,omitempty"`
	Explanation string `json:"explanation,omitempty"`
}

// ChatStreamChunk는 ProcessQuery 스트림에서 디코딩된 단일 응답 이벤트다.
type ChatStreamChunk struct {
	Type              string               `json:"type"` // initial, identifiers, data, end_of_data, half_close
	ConversationID    string               `json:"conversation_id,omitempty"`
	RequestID         string               `json:"request_id,omitempty"`
	ResponseID        string               `json:"response_id,omitempty"`
	Text              string               `json:"text,omitempty"`
	Delta             string               `json:"delta,omitempty"`
	Thoughts          string               `json:"thoughts,omitempty"`
	ThoughtsDelta     string               `json:"thoughts_delta,omitempty"`
	StatusText        string               `json:"status_text,omitempty"`
	ToolActivities    []RemoteToolActivity `json:"tool_activities,omitempty"`
	Artifacts         []TurnArtifact       `json:"artifacts,omitempty"`
	FollowUp          *FollowUpPrompt      `json:"follow_up,omitempty"`
	Elicitations      *ElicitationsGroup   `json:"elicitations,omitempty"`
	TargetDeviceName  string               `json:"target_device_name,omitempty"`
	Title             string               `json:"title,omitempty"`
	ModelAttribution  string               `json:"model,omitempty"`
	BardModeID        string               `json:"bard_mode_id,omitempty"`
	CompletenessState int32                `json:"completeness_state,omitempty"`
	TreeCompleted     bool                 `json:"done"`
}

// DecodeProcessQueryResponseFrame은 ProcessQueryResponse 단일 프레임을 디코딩한다.
func DecodeProcessQueryResponseFrame(data []byte) ([]ChatStreamChunk, error) {
	fields, err := ParseFields(data)
	if err != nil {
		return nil, fmt.Errorf("ProcessQueryResponse 파싱 실패: %w", err)
	}
	logUnknownFieldsIfDebug("ProcessQueryResponse", fields, knownProcessQueryRespFields)
	var out []ChatStreamChunk
	for _, f := range fields {
		switch f.Num {
		case extProcessQueryInitialResp:
			out = append(out, ChatStreamChunk{Type: "initial"})
		case extContinuationIdentifiersRs:
			cid, rid := decodeContinuationIdentifiers(f.Bytes)
			out = append(out, ChatStreamChunk{
				Type:           "identifiers",
				ConversationID: cid,
				RequestID:      rid,
			})
		case extClientGenerateRespStream:
			sfs, _ := ParseFields(f.Bytes)
			for _, sf := range sfs {
				switch sf.Num {
				case 1: // data: ClientGenerateResponse
					out = append(out, decodeClientGenerateResponse(sf.Bytes))
				case 2: // end_of_data
					out = append(out, ChatStreamChunk{Type: "end_of_data", TreeCompleted: true})
				}
			}
		case extHalfCloseResp:
			out = append(out, ChatStreamChunk{Type: "half_close", TreeCompleted: true})
		}
	}
	return out, nil
}

func decodeContinuationIdentifiers(data []byte) (convID, reqID string) {
	fields, _ := ParseFields(data)
	for _, f := range fields {
		if f.WireType == WireBytes {
			// ContinuationIdentifiers 또는 ResponseIdentifiers 내부 문자열 탐색
			sub, err := ParseFields(f.Bytes)
			if err == nil && len(sub) > 0 {
				for _, sf := range sub {
					if sf.Num == 1 && sf.WireType == WireBytes && strings.HasPrefix(string(sf.Bytes), "c_") {
						convID = string(sf.Bytes)
					} else if sf.Num == 2 && sf.WireType == WireBytes && strings.HasPrefix(string(sf.Bytes), "r_") {
						reqID = string(sf.Bytes)
					}
				}
			}
			s := string(f.Bytes)
			if convID == "" && strings.HasPrefix(s, "c_") {
				convID = s
			} else if reqID == "" && strings.HasPrefix(s, "r_") {
				reqID = s
			}
		}
	}
	return convID, reqID
}

func decodeClientGenerateResponse(data []byte) ChatStreamChunk {
	chunk := ChatStreamChunk{Type: "data"}
	fields, _ := ParseFields(data)
	logUnknownFieldsIfDebug("ClientGenerateResponse", fields, knownClientGenerateRespFields)
	for _, f := range fields {
		switch f.Num {
		case 1: // repeated ModelResponse (첫 번째 후보 사용)
			if chunk.ResponseID == "" && chunk.Text == "" && f.WireType == WireBytes {
				mr := decodeModelResponse(f.Bytes)
				chunk.ResponseID = mr.ResponseID
				chunk.Text = mr.Text
				chunk.Thoughts = mr.Thoughts
				if len(mr.Artifacts) > 0 {
					chunk.Artifacts = mr.Artifacts
				}
				chunk.FollowUp = mr.FollowUp
				chunk.Elicitations = mr.Elicitations
				chunk.CompletenessState = mr.CompletenessState
			}
		case 2: // ResponseIdentifiers
			chunk.ConversationID, chunk.RequestID = decodeResponseIdentifiers(f.Bytes)
		case 4: // TitleUpdate
			tfs, _ := ParseFields(f.Bytes)
			for _, tf := range tfs {
				if tf.Num == 1 && tf.WireType == WireBytes {
					chunk.Title = string(tf.Bytes)
				}
			}
		case 6: // status_update_response (StatusUpdateResponse)
			if f.WireType == WireBytes {
				statusText, activities := decodeStatusUpdateResponse(f.Bytes)
				if statusText != "" {
					chunk.StatusText = statusText
				}
				if len(activities) > 0 {
					chunk.ToolActivities = append(chunk.ToolActivities, activities...)
				}
			}
		case 9: // is_message_tree_completed
			chunk.TreeCompleted = f.Varint != 0
		case 25:
			chunk.BardModeID = string(f.Bytes)
		case 30:
			chunk.ModelAttribution = string(f.Bytes)
		case 34: // remote_device_target (SavedRemoteDeviceTarget)
			if f.WireType == WireBytes {
				rfs, _ := ParseFields(f.Bytes)
				for _, rf := range rfs {
					if rf.Num == 2 && rf.WireType == WireBytes {
						chunk.TargetDeviceName = string(rf.Bytes)
					}
				}
			}
		}
	}
	return chunk
}

func decodeStatusUpdateResponse(data []byte) (string, []RemoteToolActivity) {
	statusText, _, activities := decodeStatusUpdateResponseDetail(data)
	return statusText, activities
}

func cleanToolStatusText(raw, toolName string) string {
	s := strings.ReplaceAll(raw, "{tool_logo_placeholder}", "")
	if toolName != "" {
		s = strings.ReplaceAll(s, "{tool_name_placeholder}", toolName)
	} else {
		s = strings.ReplaceAll(s, "{tool_name_placeholder}", "")
	}
	return strings.TrimSpace(strings.Join(strings.Fields(s), " "))
}

func dedupToolActivities(in []RemoteToolActivity) []RemoteToolActivity {
	if len(in) <= 1 {
		return in
	}
	seen := make(map[string]struct{}, len(in))
	out := make([]RemoteToolActivity, 0, len(in))
	for _, a := range in {
		key := a.ID + "\x00" + a.Name + "\x00" + a.Action + "\x00" + a.Summary + "\x00" + a.Explanation
		if _, exists := seen[key]; exists {
			continue
		}
		seen[key] = struct{}{}
		out = append(out, a)
	}
	return out
}

func decodeStatusUpdateResponseDetail(data []byte) (string, string, []RemoteToolActivity) {
	fields, err := ParseFields(data)
	if err != nil {
		return "", "", nil
	}
	var statusText string
	var goalTitle string
	var activities []RemoteToolActivity
	for _, f := range fields {
		switch f.Num {
		case 1: // text
			if f.WireType == WireBytes && len(f.Bytes) > 0 {
				statusText = cleanToolStatusText(string(f.Bytes), "")
			}
		case 2: // 일반 대화 도구 상태 (예: Google Search 등)
			if f.WireType != WireBytes {
				continue
			}
			tfs, _ := ParseFields(f.Bytes)
			var act RemoteToolActivity
			for _, tf := range tfs {
				switch tf.Num {
				case 1:
					if tf.WireType == WireBytes {
						act.ID = string(tf.Bytes)
					}
				case 2:
					if tf.WireType == WireBytes {
						mfs, _ := ParseFields(tf.Bytes)
						for _, mf := range mfs {
							if mf.WireType != WireBytes {
								continue
							}
							switch mf.Num {
							case 3:
								act.Name = strings.TrimSpace(string(mf.Bytes))
							case 6:
								act.Summary = string(mf.Bytes)
							}
						}
					}
				}
			}
			if act.Name != "" || act.Summary != "" {
				if act.Name == "" {
					act.Name = "도구"
				}
				act.Summary = cleanToolStatusText(act.Summary, act.Name)
				activities = append(activities, act)
				if statusText == "" && act.Summary != "" {
					statusText = act.Summary
				}
			}
		case 13: // remy_processing_state
			if f.WireType != WireBytes {
				continue
			}
			rfs, _ := ParseFields(f.Bytes)
			for _, rf := range rfs {
				switch rf.Num {
				case 1: // repeated RemyToolCall
					if rf.WireType != WireBytes {
						continue
					}
					tfs, _ := ParseFields(rf.Bytes)
					var act RemoteToolActivity
					for _, tf := range tfs {
						if tf.WireType != WireBytes {
							continue
						}
						switch tf.Num {
						case 1:
							act.Explanation = string(tf.Bytes)
						case 2:
							act.Summary = string(tf.Bytes)
						case 3:
							act.Action = string(tf.Bytes)
						case 5:
							act.Name = strings.TrimSpace(string(tf.Bytes))
						case 9:
							if act.ID == "" {
								act.ID = string(tf.Bytes)
							}
						case 10:
							act.ID = string(tf.Bytes)
						}
					}
					if act.Name != "" || act.Summary != "" || act.Action != "" {
						act.Summary = cleanToolStatusText(act.Summary, act.Name)
						act.Explanation = cleanToolStatusText(act.Explanation, act.Name)
						activities = append(activities, act)
					}
				case 4: // model_thought_summary
					if statusText == "" && rf.WireType == WireBytes && len(rf.Bytes) > 0 {
						statusText = cleanToolStatusText(string(rf.Bytes), "")
					}
				case 9: // goal_title (예약 작업/태스크 제목)
					if rf.WireType == WireBytes && len(rf.Bytes) > 0 {
						title := strings.TrimSpace(string(rf.Bytes))
						if goalTitle == "" {
							goalTitle = title
						}
						if statusText == "" {
							statusText = title
						}
					}
				case 10: // summary
					if statusText == "" && rf.WireType == WireBytes && len(rf.Bytes) > 0 {
						statusText = cleanToolStatusText(string(rf.Bytes), "")
					}
				}
			}
		}
	}
	if len(activities) > 0 && strings.Contains(statusText, "{tool_") {
		statusText = cleanToolStatusText(statusText, activities[0].Name)
	}
	return statusText, goalTitle, dedupToolActivities(activities)
}

// EncodeAbortGenerationRequest는 진행 중인 생성을 중단하는 요청을 인코딩한다.
func EncodeAbortGenerationRequest(metadata []byte, conversationID, requestID string) []byte {
	ids := NewBuilder()
	ids.AppendString(1, conversationID)
	ids.AppendString(2, requestID)

	b := NewBuilder()
	b.AppendMessage(1, ids.Bytes())
	b.AppendBool(3, true) // partial_response_already_presented
	b.AppendMessage(4, metadata)
	return b.Bytes()
}

// EncodeDeleteConversationRequest는 대화 삭제 요청을 인코딩한다.
func EncodeDeleteConversationRequest(metadata []byte, conversationID string) []byte {
	b := NewBuilder()
	b.AppendString(1, conversationID)
	b.AppendMessage(2, metadata)
	return b.Bytes()
}

// EncodeUpdateConversationTitleRequest는 대화 제목 변경 요청을 인코딩한다.
func EncodeUpdateConversationTitleRequest(metadata []byte, conversationID, title string) []byte {
	conv := NewBuilder()
	conv.AppendString(1, conversationID)
	conv.AppendString(2, title)

	mask := NewBuilder()
	mask.AppendString(1, "title")

	b := NewBuilder()
	b.AppendMessage(1, conv.Bytes())
	b.AppendMessage(2, mask.Bytes())
	b.AppendMessage(4, metadata)
	return b.Bytes()
}

// EncodeUpdateConversationPinnedRequest는 대화 고정 여부 변경 요청을 인코딩한다.
func EncodeUpdateConversationPinnedRequest(metadata []byte, conversationID string, pinned bool) []byte {
	conv := NewBuilder()
	conv.AppendString(1, conversationID)
	conv.AppendBoolAlways(3, pinned)

	mask := NewBuilder()
	mask.AppendString(1, "pinned")

	b := NewBuilder()
	b.AppendMessage(1, conv.Bytes())
	b.AppendMessage(2, mask.Bytes())
	b.AppendMessage(4, metadata)
	return b.Bytes()
}

// EncodeUpdateConversationUnreadRequest는 대화 읽지 않음 표시 변경 요청을 인코딩한다.
func EncodeUpdateConversationUnreadRequest(metadata []byte, conversationID string, unread bool) []byte {
	unreadMeta := NewBuilder()
	unreadMeta.AppendBoolAlways(2, unread)

	conv := NewBuilder()
	conv.AppendString(1, conversationID)
	conv.AppendMessage(8, unreadMeta.Bytes())

	mask := NewBuilder()
	mask.AppendString(1, "unread_metadata")

	b := NewBuilder()
	b.AppendMessage(1, conv.Bytes())
	b.AppendMessage(2, mask.Bytes())
	b.AppendMessage(4, metadata)
	return b.Bytes()
}

// BardModeOption은 사용자 계정에서 선택 가능한 Gemini 모델/모드 항목이다.
type BardModeOption struct {
	ID                   string `json:"id"`
	Title                string `json:"title"`
	Description          string `json:"description,omitempty"`
	TitleForPicker       string `json:"title_for_picker,omitempty"`
	DescriptionForPicker string `json:"description_for_picker,omitempty"`
	ModeName             string `json:"mode_name,omitempty"`
	FullModeName         string `json:"full_mode_name,omitempty"`
	BardMode             int32  `json:"bard_mode,omitempty"`
	ModeCategory         int32  `json:"mode_category,omitempty"`
	InPicker             bool   `json:"in_picker,omitempty"`
	RequiresAdvanced     bool   `json:"requires_advanced,omitempty"`
	SupportsThinking     bool   `json:"supports_thinking,omitempty"`
}

// UserStatusResult는 GetUserStatus 응답에서 추출한 모델 선택기 및 구독 티어 정보다.
type UserStatusResult struct {
	ModePickerTitle string           `json:"mode_picker_title,omitempty"`
	Tier            string           `json:"tier,omitempty"`
	Models          []BardModeOption `json:"models"`
}

// EncodeGetUserStatusRequest는 계정 상태 및 사용 가능한 모델 목록 조회 요청을 인코딩한다.
func EncodeGetUserStatusRequest(metadata []byte) []byte {
	b := NewBuilder()
	b.AppendMessage(1, metadata)
	return b.Bytes()
}

type pickerCategory struct {
	modeIDs          []string
	requiresAdvanced bool
}

// DecodeGetUserStatusResponse는 GetUserStatus 응답에서 모델 목록과 피커 구성을 디코딩한다.
func DecodeGetUserStatusResponse(data []byte) (*UserStatusResult, error) {
	fields, err := ParseFields(data)
	if err != nil {
		return nil, err
	}
	res := &UserStatusResult{
		Models: []BardModeOption{},
	}
	byID := map[string]BardModeOption{}
	var rawOrder []string
	var categories []pickerCategory
	var tierCode uint64
	hasUltraPolicy := false
	hasAdvanced := false

	for _, f := range fields {
		switch f.Num {
		case 17: // repeated RobinBardModeConfig
			if f.WireType != WireBytes {
				continue
			}
			m := decodeBardModeConfig(f.Bytes)
			if m.ID != "" {
				if _, exists := byID[m.ID]; !exists {
					rawOrder = append(rawOrder, m.ID)
				}
				byID[m.ID] = m
			}
		case 18: // repeated BardModePickerCategory
			if f.WireType != WireBytes {
				continue
			}
			cat := decodePickerCategory(f.Bytes)
			if cat.requiresAdvanced {
				hasAdvanced = true
			}
			categories = append(categories, cat)
		case 21:
			if f.WireType == WireVarint {
				tierCode = f.Varint
			}
		case 27: // mode_picker_title
			if f.WireType == WireBytes {
				res.ModePickerTitle = string(f.Bytes)
			}
		case 34:
			if f.WireType == WireBytes && strings.Contains(strings.ToLower(string(f.Bytes)), "ultra") {
				hasUltraPolicy = true
			}
		}
	}

	if hasUltraPolicy || tierCode >= 5 {
		res.Tier = "Ultra"
	} else if hasAdvanced || tierCode >= 2 {
		res.Tier = "Pro"
	}

	seen := map[string]bool{}
	for _, cat := range categories {
		for _, id := range cat.modeIDs {
			m, ok := byID[id]
			if !ok || seen[id] {
				continue
			}
			m.InPicker = true
			m.RequiresAdvanced = cat.requiresAdvanced
			res.Models = append(res.Models, m)
			seen[id] = true
		}
	}
	for _, id := range rawOrder {
		if seen[id] {
			continue
		}
		res.Models = append(res.Models, byID[id])
	}
	return res, nil
}

// BotSummary는 사용자 계정의 Gem(커스텀/프리셋 봇) 요약 정보다.
type BotSummary struct {
	ID          string   `json:"id"`
	Name        string   `json:"name"`
	Description string   `json:"description,omitempty"`
	Starters    []string `json:"starters,omitempty"`
	IsCustom    bool     `json:"is_custom,omitempty"`
}

// EncodeListBotsRequest는 Gem(봇) 목록 조회 요청을 인코딩한다.
func EncodeListBotsRequest(metadata []byte) []byte {
	b := NewBuilder()
	b.AppendMessage(1, metadata)
	return b.Bytes()
}

// DecodeListBotsResponse는 ListBots 응답에서 Gem 목록을 디코딩한다.
func DecodeListBotsResponse(data []byte) ([]BotSummary, error) {
	fields, err := ParseFields(data)
	if err != nil {
		return nil, err
	}
	var bots []BotSummary
	for _, f := range fields {
		if f.Num != 2 || f.WireType != WireBytes {
			continue
		}
		bf, err := ParseFields(f.Bytes)
		if err != nil {
			continue
		}
		var bot BotSummary
		for _, sub := range bf {
			switch sub.Num {
			case 1:
				if sub.WireType == WireBytes {
					bot.ID = string(sub.Bytes)
				}
			case 2: // metadata
				if sub.WireType == WireBytes {
					mf, _ := ParseFields(sub.Bytes)
					for _, m := range mf {
						switch m.Num {
						case 1:
							if m.WireType == WireBytes {
								bot.Name = string(m.Bytes)
							}
						case 2:
							if m.WireType == WireBytes {
								bot.Description = string(m.Bytes)
							}
						case 4:
							if m.WireType == WireBytes && len(m.Bytes) > 0 {
								bot.Starters = append(bot.Starters, string(m.Bytes))
							}
						case 6:
							if m.WireType == WireVarint && m.Varint == 1 {
								bot.IsCustom = true
							}
						}
					}
				}
			case 4:
				if sub.WireType == WireVarint && sub.Varint == 1 {
					bot.IsCustom = true
				}
			}
		}
		if bot.ID != "" && bot.Name != "" {
			bots = append(bots, bot)
		}
	}
	return bots, nil
}

// SearchConversationHit는 서버 대화 이력 검색 결과 단일 항목이다.
type SearchConversationHit struct {
	ConversationID string `json:"conversation_id"`
	Title          string `json:"title"`
	Snippet        string `json:"snippet,omitempty"`
	RequestID      string `json:"request_id,omitempty"`
}

// EncodeSearchConversationsRequest는 대화 이력 전문 검색 요청을 인코딩한다.
func EncodeSearchConversationsRequest(metadata []byte, query string, pageSize int32) []byte {
	if pageSize <= 0 {
		pageSize = 20
	}
	b := NewBuilder()
	b.AppendString(1, query)
	b.AppendInt32(2, pageSize)
	b.AppendMessage(4, metadata)
	return b.Bytes()
}

// DecodeSearchConversationsResponse는 SearchConversationHistory 응답을 디코딩한다.
func DecodeSearchConversationsResponse(data []byte) ([]SearchConversationHit, error) {
	fields, err := ParseFields(data)
	if err != nil {
		return nil, err
	}
	var hits []SearchConversationHit
	for _, f := range fields {
		if f.Num != 1 || f.WireType != WireBytes {
			continue
		}
		hf, err := ParseFields(f.Bytes)
		if err != nil {
			continue
		}
		var hit SearchConversationHit
		for _, sub := range hf {
			switch sub.Num {
			case 1:
				if sub.WireType == WireBytes {
					cf, _ := ParseFields(sub.Bytes)
					for _, c := range cf {
						if c.Num == 1 && c.WireType == WireBytes {
							hit.ConversationID = string(c.Bytes)
						} else if c.Num == 2 && c.WireType == WireBytes {
							hit.Title = string(c.Bytes)
						}
					}
				}
			case 3:
				if sub.WireType == WireBytes {
					sf, _ := ParseFields(sub.Bytes)
					for _, s := range sf {
						if s.Num == 2 && s.WireType == WireBytes && hit.Snippet == "" {
							hit.Snippet = strings.TrimSpace(string(s.Bytes))
						} else if s.Num == 5 && s.WireType == WireBytes {
							hit.RequestID = string(s.Bytes)
						}
					}
				}
			}
		}
		if hit.ConversationID != "" {
			hits = append(hits, hit)
		}
	}
	return hits, nil
}

func decodeBardModeConfig(data []byte) BardModeOption {
	var m BardModeOption
	fields, _ := ParseFields(data)
	for _, f := range fields {
		switch f.Num {
		case 1:
			m.ID = string(f.Bytes)
		case 2:
			m.Title = string(f.Bytes)
		case 3:
			m.Description = string(f.Bytes)
		case 4:
			m.BardMode = int32(f.Varint)
		case 18:
			m.ModeName = string(f.Bytes)
		case 19:
			m.TitleForPicker = string(f.Bytes)
		case 20:
			m.DescriptionForPicker = string(f.Bytes)
		case 23:
			m.ModeCategory = int32(f.Varint)
		case 26:
			m.FullModeName = string(f.Bytes)
		}
	}
	// 자동 모드(mode_category=6)를 제외한 일반 모델은 사고 수준 선택 지원
	m.SupportsThinking = m.ModeCategory != 6
	return m
}

func decodePickerCategory(data []byte) pickerCategory {
	var c pickerCategory
	fields, _ := ParseFields(data)
	for _, f := range fields {
		switch f.Num {
		case 1:
			if f.WireType == WireBytes {
				c.modeIDs = append(c.modeIDs, string(f.Bytes))
			}
		case 2:
			if f.WireType == WireVarint {
				c.requiresAdvanced = f.Varint != 0
			}
		}
	}
	return c
}
