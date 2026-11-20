package proto

import (
	"fmt"
	"math"
	"strings"
)

// RemoteNotebook은 서버에 저장된 Gemini 노트북 항목이다.
type RemoteNotebook struct {
	ID            string `json:"id"`
	Title         string `json:"title"`
	UpdatedAtUnix int64  `json:"updated_at_unix"`
	SourceCount   int    `json:"source_count"`
}

// EncodeListGeminiNotebooksRequest는 서버 노트북 목록 조회 요청을 인코딩한다.
func EncodeListGeminiNotebooksRequest(metadata []byte) []byte {
	b := NewBuilder()
	b.AppendMessage(1, metadata)
	return b.Bytes()
}

// DecodeListGeminiNotebooksResponse는 서버 노트북 목록 응답을 디코딩한다.
func DecodeListGeminiNotebooksResponse(data []byte) ([]RemoteNotebook, error) {
	fields, err := ParseFields(data)
	if err != nil {
		return nil, err
	}
	out := []RemoteNotebook{}
	for _, f := range fields {
		if f.Num != 1 || f.WireType != WireBytes {
			continue
		}
		nf, err := ParseFields(f.Bytes)
		if err != nil {
			continue
		}
		var nb RemoteNotebook
		for _, sf := range nf {
			switch sf.Num {
			case 1:
				if sf.WireType == WireBytes {
					nb.ID = string(sf.Bytes)
				}
			case 2:
				if sf.WireType == WireBytes {
					nb.Title = strings.TrimSpace(string(sf.Bytes))
				}
			case 3:
				if sf.WireType == WireBytes {
					nb.UpdatedAtUnix = decodeTimestampSeconds(sf.Bytes)
				}
			case 4:
				if sf.WireType == WireVarint {
					nb.SourceCount = int(sf.Varint)
				}
			}
		}
		if nb.ID != "" {
			if nb.Title == "" {
				nb.Title = "제목 없는 노트북"
			}
			out = append(out, nb)
		}
	}
	return out, nil
}

// EncodeCreateGbhNotebookRequest는 서버 노트북 생성 요청을 인코딩한다.
func EncodeCreateGbhNotebookRequest(metadata []byte, title string) []byte {
	b := NewBuilder()
	b.AppendMessage(1, metadata)
	b.AppendString(2, strings.TrimSpace(title))
	return b.Bytes()
}

// DecodeCreateGbhNotebookResponse는 서버 노트북 생성 응답에서 생성된 노트북 정보를 디코딩한다.
func DecodeCreateGbhNotebookResponse(data []byte) (*RemoteNotebook, error) {
	fields, err := ParseFields(data)
	if err != nil {
		return nil, err
	}
	nb := &RemoteNotebook{}
	for _, f := range fields {
		switch f.Num {
		case 1:
			if f.WireType == WireBytes {
				s := string(f.Bytes)
				if strings.HasPrefix(s, "notebooks/") {
					nb.ID = s
				} else {
					sub, _ := ParseFields(f.Bytes)
					for _, sf := range sub {
						switch sf.Num {
						case 1:
							if sf.WireType == WireBytes {
								nb.ID = string(sf.Bytes)
							}
						case 2:
							if sf.WireType == WireBytes {
								nb.Title = string(sf.Bytes)
							}
						}
					}
				}
			}
		case 2:
			if f.WireType == WireBytes && nb.Title == "" {
				nb.Title = string(f.Bytes)
			}
		}
	}
	if nb.ID == "" {
		return nil, fmt.Errorf("생성된 노트북 ID를 응답에서 찾을 수 없습니다")
	}
	return nb, nil
}

// RemoteSchedule은 서버에 등록된 Gemini 예약 작업(RemySchedule) 정보다.
type RemoteSchedule struct {
	ID                 string `json:"id"`
	Title              string `json:"title"`
	Prompt             string `json:"prompt"`
	ScheduleType       string `json:"schedule_type"`
	TimeOfDay          string `json:"time_of_day"`
	ScheduleDesc       string `json:"schedule_desc,omitempty"`
	TimeZone           string `json:"time_zone,omitempty"`
	Enabled            bool   `json:"enabled"`
	LastRunUnix        int64  `json:"last_run_unix,omitempty"`
	LastConversationID string `json:"last_conversation_id,omitempty"`
}

// RecommendedSchedule은 서버가 제안하는 추천 예약 작업 템플릿이다.
type RecommendedSchedule struct {
	ID          string `json:"id"`
	Title       string `json:"title"`
	Description string `json:"description,omitempty"`
	Prompt      string `json:"prompt,omitempty"`
}

// EncodeListRemySchedulesRequest는 서버 예약 작업 목록 조회 요청을 인코딩한다.
func EncodeListRemySchedulesRequest(metadata []byte) []byte {
	b := NewBuilder()
	b.AppendMessage(1, metadata)
	return b.Bytes()
}

// DecodeListRemySchedulesResponse는 서버 예약 작업 목록 응답을 디코딩한다.
func DecodeListRemySchedulesResponse(data []byte) ([]RemoteSchedule, error) {
	fields, err := ParseFields(data)
	if err != nil {
		return nil, err
	}
	out := []RemoteSchedule{}
	for _, f := range fields {
		if f.Num != 1 || f.WireType != WireBytes {
			continue
		}
		sf, err := ParseFields(f.Bytes)
		if err != nil {
			continue
		}
		item := RemoteSchedule{
			ScheduleType: "daily",
			TimeOfDay:    "09:00",
			Enabled:      true,
		}
		hasExplicitEnabled := false
		var stateVal uint64
		for _, sub := range sf {
			switch sub.Num {
			case 1:
				if sub.WireType == WireBytes {
					item.ID = string(sub.Bytes)
				}
			case 2:
				if sub.WireType == WireBytes {
					item.Title = strings.TrimSpace(string(sub.Bytes))
				}
			case 3:
				if sub.WireType == WireVarint {
					stateVal = sub.Varint
				}
			case 4: // daily schedule: f1 -> {f1: hour, f2: minute}
				item.ScheduleType = "daily"
				if h, m, ok := decodeScheduleHourMinute(sub.Bytes, 1); ok {
					item.TimeOfDay = fmt.Sprintf("%02d:%02d", h, m)
				}
			case 5: // weekly schedule: f2 -> {f1: hour, f2: minute}
				item.ScheduleType = "weekly"
				if h, m, ok := decodeScheduleHourMinute(sub.Bytes, 2); ok {
					item.TimeOfDay = fmt.Sprintf("%02d:%02d", h, m)
				}
			case 9:
				if sub.WireType == WireBytes {
					item.TimeZone = string(sub.Bytes)
				}
			case 10:
				if sub.WireType == WireBytes {
					item.Prompt = strings.TrimSpace(string(sub.Bytes))
				}
			case 12:
				if sub.WireType == WireVarint {
					hasExplicitEnabled = true
					item.Enabled = sub.Varint != 0
				}
			case 13:
				if sub.WireType == WireBytes {
					item.LastConversationID = string(sub.Bytes)
				}
			case 23:
				if sub.WireType == WireBytes {
					item.LastRunUnix = decodeTimestampSeconds(sub.Bytes)
				}
			case 25:
				if sub.WireType == WireBytes {
					item.ScheduleDesc = strings.TrimSpace(string(sub.Bytes))
				}
			}
		}
		if !hasExplicitEnabled && stateVal != 0 {
			item.Enabled = stateVal == 1
		}
		if item.ID != "" {
			if item.Title == "" {
				item.Title = "예약된 작업"
			}
			out = append(out, item)
		}
	}
	return out, nil
}

func decodeScheduleHourMinute(data []byte, timeField uint32) (int, int, bool) {
	fields, err := ParseFields(data)
	if err != nil {
		return 0, 0, false
	}
	for _, f := range fields {
		if f.Num == timeField && f.WireType == WireBytes {
			tf, err := ParseFields(f.Bytes)
			if err != nil {
				continue
			}
			var h, m int
			var found bool
			for _, sub := range tf {
				if sub.WireType != WireVarint {
					continue
				}
				switch sub.Num {
				case 1:
					h = int(sub.Varint)
					found = true
				case 2:
					m = int(sub.Varint)
				}
			}
			if found {
				return h, m, true
			}
		}
	}
	return 0, 0, false
}

// EncodeListRecommendedSchedulesRequest는 추천 예약 작업 목록 조회 요청을 인코딩한다.
func EncodeListRecommendedSchedulesRequest(metadata []byte) []byte {
	b := NewBuilder()
	b.AppendMessage(1, metadata)
	return b.Bytes()
}

// DecodeListRecommendedSchedulesResponse는 추천 예약 작업 목록 응답을 디코딩한다.
func DecodeListRecommendedSchedulesResponse(data []byte) ([]RecommendedSchedule, error) {
	fields, err := ParseFields(data)
	if err != nil {
		return nil, err
	}
	out := []RecommendedSchedule{}
	for _, f := range fields {
		if f.Num != 1 || f.WireType != WireBytes {
			continue
		}
		rf, err := ParseFields(f.Bytes)
		if err != nil {
			continue
		}
		var rec RecommendedSchedule
		for _, sub := range rf {
			switch sub.Num {
			case 1: // Schedule 템플릿
				if sub.WireType == WireBytes {
					sf, _ := ParseFields(sub.Bytes)
					for _, s := range sf {
						switch s.Num {
						case 1:
							if s.WireType == WireBytes {
								rec.ID = string(s.Bytes)
							}
						case 2:
							if s.WireType == WireBytes {
								rec.Title = strings.TrimSpace(string(s.Bytes))
							}
						case 10:
							if s.WireType == WireBytes && rec.Prompt == "" {
								rec.Prompt = strings.TrimSpace(string(s.Bytes))
							}
						}
					}
				}
			case 3:
				if sub.WireType == WireBytes {
					rec.Description = strings.TrimSpace(string(sub.Bytes))
				}
			case 4:
				if sub.WireType == WireBytes {
					rec.Prompt = strings.TrimSpace(string(sub.Bytes))
				}
			}
		}
		if rec.Title != "" || rec.ID != "" {
			if rec.ID == "" {
				rec.ID = rec.Title
			}
			out = append(out, rec)
		}
	}
	return out, nil
}

// EncodeDeleteRemyScheduleRequest는 서버 예약 작업 삭제 요청을 인코딩한다.
func EncodeDeleteRemyScheduleRequest(metadata []byte, scheduleID string) []byte {
	b := NewBuilder()
	b.AppendMessage(1, metadata)
	b.AppendString(2, scheduleID)
	return b.Bytes()
}

// EncodeExecuteRemyScheduleRequest는 서버 예약 작업 즉시 실행 요청을 인코딩한다.
func EncodeExecuteRemyScheduleRequest(metadata []byte, scheduleID string) []byte {
	b := NewBuilder()
	b.AppendMessage(1, metadata)
	b.AppendString(2, scheduleID)
	return b.Bytes()
}

// EncodeAddRecommendedScheduleRequest는 추천 예약 작업 추가 요청을 인코딩한다.
func EncodeAddRecommendedScheduleRequest(metadata []byte, templateID string) []byte {
	b := NewBuilder()
	b.AppendMessage(1, metadata)
	b.AppendString(2, templateID)
	return b.Bytes()
}

// AppArtifact는 서버 라이브러리(문서·Canvas·생성 미디어) 단일 항목이다.
type AppArtifact struct {
	ID             string `json:"id"`
	Kind           string `json:"kind"` // "document" 또는 "image"
	Title          string `json:"title"`
	URL            string `json:"url,omitempty"`
	ContentPreview string `json:"content_preview,omitempty"`
	ConversationID string `json:"conversation_id,omitempty"`
	RequestID      string `json:"request_id,omitempty"`
	ResponseID     string `json:"response_id,omitempty"`
	CreatedAtUnix  int64  `json:"created_at_unix"`
}

// EncodeListGeminiAppArtifactsRequest는 라이브러리 아티팩트(문서 f3=meta / 미디어 f4=meta) 조회 요청을 인코딩한다.
func EncodeListGeminiAppArtifactsRequest(metadata []byte, pageSize int32, isMedia bool) []byte {
	if pageSize <= 0 {
		pageSize = 24
	}
	b := NewBuilder()
	b.AppendInt32(1, pageSize)
	if isMedia {
		b.AppendMessage(4, metadata)
	} else {
		b.AppendMessage(3, metadata)
	}
	return b.Bytes()
}

// DecodeListGeminiAppArtifactsResponse는 라이브러리 아티팩트 조회 응답을 디코딩한다.
func DecodeListGeminiAppArtifactsResponse(data []byte, isMedia bool) ([]AppArtifact, error) {
	fields, err := ParseFields(data)
	if err != nil {
		return nil, err
	}
	out := []AppArtifact{}
	for _, f := range fields {
		if f.Num != 1 || f.WireType != WireBytes {
			continue
		}
		af, err := ParseFields(f.Bytes)
		if err != nil {
			continue
		}
		item := AppArtifact{Kind: "document"}
		if isMedia {
			item.Kind = "image"
		}
		var artifactType uint64
		for _, sub := range af {
			switch sub.Num {
			case 1: // {f1: conv_id, f2: req_id}
				if sub.WireType == WireBytes {
					item.ConversationID, item.RequestID = decodeResponseIdentifiers(sub.Bytes)
				}
			case 2: // Timestamp
				if sub.WireType == WireBytes {
					item.CreatedAtUnix = decodeTimestampSeconds(sub.Bytes)
				}
			case 3: // artifact type (1|2: media, 6: canvas document)
				if sub.WireType == WireVarint {
					artifactType = sub.Varint
				}
			case 4:
				if sub.WireType == WireBytes {
					item.Title = strings.TrimSpace(string(sub.Bytes))
				}
			case 5: // payload: f1=content_preview (문서), f2=image_url (미디어)
				if sub.WireType == WireBytes {
					pf, _ := ParseFields(sub.Bytes)
					for _, p := range pf {
						if p.WireType != WireBytes {
							continue
						}
						switch p.Num {
						case 1:
							if item.ContentPreview == "" {
								s := strings.TrimSpace(string(p.Bytes))
								if len([]rune(s)) > 300 {
									s = string([]rune(s)[:300]) + "…"
								}
								item.ContentPreview = s
							}
						case 2:
							if item.URL == "" {
								item.URL = strings.TrimSpace(string(p.Bytes))
							}
						}
					}
				}
			case 6:
				if sub.WireType == WireBytes {
					item.ResponseID = string(sub.Bytes)
				}
			}
		}
		if artifactType == 1 || artifactType == 2 || item.URL != "" {
			item.Kind = "image"
		} else if artifactType == 6 {
			item.Kind = "document"
		}
		if item.Title == "" {
			if item.Kind == "image" {
				item.Title = "생성된 이미지"
			} else {
				item.Title = "Canvas 문서"
			}
		}
		item.ID = fmt.Sprintf("art_%s_%s_%s", item.Kind, item.ConversationID, item.ResponseID)
		if item.ConversationID != "" || item.URL != "" || item.ContentPreview != "" {
			out = append(out, item)
		}
	}
	return out, nil
}

// UsageBucket은 계정 사용량 한도 단일 버킷(현재 세션/주간 한도)이다.
type UsageBucket struct {
	BucketType   int32   `json:"bucket_type"` // 1: 현재 사용량, 2: 주간 한도
	Label        string  `json:"label"`
	UsedRatio    float32 `json:"used_ratio"` // 0.0 ~ 1.0
	ResetAtUnix  int64   `json:"reset_at_unix,omitempty"`
	RemainingVal int64   `json:"remaining_val,omitempty"`
	MaxVal       int64   `json:"max_val,omitempty"`
}

// UsageInfo는 GetUsageInfo 응답에서 디코딩한 사용량 요약 정보다.
type UsageInfo struct {
	Buckets    []UsageBucket `json:"buckets"`
	NoticeText string        `json:"notice_text,omitempty"`
	SupportURL string        `json:"support_url,omitempty"`
}

// EncodeGetUsageInfoRequest는 계정 사용량 한도 조회 요청을 인코딩한다.
func EncodeGetUsageInfoRequest(metadata []byte) []byte {
	b := NewBuilder()
	b.AppendMessage(1, metadata)
	return b.Bytes()
}

// DecodeGetUsageInfoResponse는 계정 사용량 한도 응답을 디코딩한다.
func DecodeGetUsageInfoResponse(data []byte) (*UsageInfo, error) {
	fields, err := ParseFields(data)
	if err != nil {
		return nil, err
	}
	info := &UsageInfo{Buckets: []UsageBucket{}}
	for _, f := range fields {
		switch f.Num {
		case 3: // repeated UsageBucket
			if f.WireType != WireBytes {
				continue
			}
			bf, err := ParseFields(f.Bytes)
			if err != nil {
				continue
			}
			var b UsageBucket
			for _, sub := range bf {
				switch sub.Num {
				case 1:
					if sub.WireType == WireVarint {
						b.BucketType = int32(sub.Varint)
					}
				case 2:
					if sub.WireType == WireBytes {
						df, _ := ParseFields(sub.Bytes)
						for _, d := range df {
							switch d.Num {
							case 1:
								if d.WireType == WireBytes {
									b.ResetAtUnix = decodeTimestampSeconds(d.Bytes)
								}
							case 2:
								if d.WireType == WireFixed32 {
									b.UsedRatio = math.Float32frombits(d.Fixed32)
								}
							case 3:
								if d.WireType == WireBytes {
									b.Label = strings.TrimSpace(string(d.Bytes))
								}
							case 5:
								if d.WireType == WireVarint {
									b.RemainingVal = int64(d.Varint)
								}
							case 6:
								if d.WireType == WireVarint {
									b.MaxVal = int64(d.Varint)
								}
							}
						}
					}
				}
			}
			if b.Label == "" {
				switch b.BucketType {
				case 1:
					b.Label = "현재 사용량"
				case 2:
					b.Label = "주간 한도"
				default:
					b.Label = "사용량 한도"
				}
			}
			ratio := float64(b.UsedRatio)
			if math.IsNaN(ratio) || math.IsInf(ratio, 0) || b.UsedRatio < 0 {
				b.UsedRatio = 0
			} else if b.UsedRatio > 1 {
				b.UsedRatio = 1
			}
			info.Buckets = append(info.Buckets, b)
		case 4: // 안내 문구 & 지원 링크
			if f.WireType == WireBytes {
				nf, _ := ParseFields(f.Bytes)
				for _, n := range nf {
					if n.WireType != WireBytes {
						continue
					}
					switch n.Num {
					case 1:
						info.NoticeText = strings.TrimSpace(string(n.Bytes))
					case 2:
						info.SupportURL = strings.TrimSpace(string(n.Bytes))
					}
				}
			}
		}
	}
	return info, nil
}

// EncodeCreateSharedConversationRequest는 대화 공유 링크 생성 요청을 인코딩한다.
func EncodeCreateSharedConversationRequest(metadata []byte, conversationID, title string) []byte {
	convTarget := NewBuilder()
	convTarget.AppendString(1, conversationID)

	target := NewBuilder()
	target.AppendMessage(1, convTarget.Bytes())

	b := NewBuilder()
	b.AppendMessage(1, target.Bytes())
	b.AppendString(2, strings.TrimSpace(title))
	b.AppendMessage(4, metadata)
	return b.Bytes()
}

// DecodeCreateSharedConversationResponse는 공유 링크 생성 응답에서 공유 URL/ID를 추출한다.
func DecodeCreateSharedConversationResponse(data []byte) (string, error) {
	fields, err := ParseFields(data)
	if err != nil {
		return "", err
	}
	var shareID, shareURL string
	for _, f := range fields {
		if f.WireType != WireBytes {
			continue
		}
		s := strings.TrimSpace(string(f.Bytes))
		if strings.HasPrefix(s, "https://") {
			shareURL = s
			break
		}
		if f.Num == 1 && s != "" && !strings.ContainsAny(s, "\x00\n\r") {
			shareID = s
		}
		if sub, serr := ParseFields(f.Bytes); serr == nil {
			for _, sf := range sub {
				if sf.WireType == WireBytes {
					ss := strings.TrimSpace(string(sf.Bytes))
					if strings.HasPrefix(ss, "https://") {
						shareURL = ss
						break
					}
				}
			}
		}
	}
	if shareURL != "" {
		return shareURL, nil
	}
	if shareID != "" {
		return "https://gemini.google.com/share/" + shareID, nil
	}
	return "", fmt.Errorf("공유 URL을 응답에서 찾을 수 없습니다")
}

// EncodeWriteFeedbackRequest는 응답 좋아요/싫어요 피드백 전송 요청을 인코딩한다.
func EncodeWriteFeedbackRequest(metadata []byte, conversationID, requestID, responseID string, positive bool, comment string) []byte {
	ids := NewBuilder()
	ids.AppendString(1, conversationID)
	ids.AppendString(2, requestID)
	ids.AppendString(3, responseID)

	b := NewBuilder()
	b.AppendMessage(1, ids.Bytes())
	if positive {
		b.AppendInt32(2, 1)
	} else {
		b.AppendInt32(2, 2)
	}
	b.AppendString(3, strings.TrimSpace(comment))
	b.AppendMessage(4, metadata)
	return b.Bytes()
}
