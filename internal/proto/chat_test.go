package proto

import (
	"testing"
)

func TestEncodeProcessQueryFramesAndDecodeResponse(t *testing.T) {
	meta := EncodeChatRequestMetadata("ko-KR", "Asia/Seoul", "test-device-id")
	f1, f2, f3 := EncodeProcessQueryFrames(meta, ChatQueryParams{
		Text:           "안녕하세요",
		ConversationID: "c_123",
		Temporary:      true,
	})

	// Frame 1: turn_sequence_number(1)=1, ProcessQueryInitialRequest(1000) 내부에 1001/1004/1010 중첩 확인
	p1, err := ParseFields(f1)
	if err != nil {
		t.Fatalf("Frame 1 파싱 실패: %v", err)
	}
	var initBytes []byte
	for _, f := range p1 {
		if f.Num == extProcessQueryInitialReq {
			initBytes = f.Bytes
		}
	}
	if len(initBytes) == 0 {
		t.Fatalf("Frame 1에 ext 1000(initial) 누락")
	}
	initFields, err := ParseFields(initBytes)
	if err != nil {
		t.Fatalf("InitialRequest 파싱 실패: %v", err)
	}
	seen := map[uint32]bool{}
	for _, f := range initFields {
		seen[f.Num] = true
	}
	for _, reqExt := range []uint32{1, extClientGenerateFeature, extPrefetchInputFeature, extHalfCloseFeature} {
		if !seen[reqExt] {
			t.Fatalf("InitialRequest에 필수 필드/확장 %d 누락", reqExt)
		}
	}

	// Frame 2: PrefetchInputRequest(1007).query(1) == "안녕하세요"
	p2, err := ParseFields(f2)
	if err != nil {
		t.Fatalf("Frame 2 파싱 실패: %v", err)
	}
	var gotQuery string
	for _, f := range p2 {
		if f.Num == extPrefetchInputReq {
			sub, _ := ParseFields(f.Bytes)
			for _, sf := range sub {
				if sf.Num == 1 {
					gotQuery = string(sf.Bytes)
				}
			}
		}
	}
	if gotQuery != "안녕하세요" {
		t.Fatalf("Frame 2 질의 불일치: got=%q", gotQuery)
	}

	// Frame 3: PrefetchInputRequest(1007).commit(2) == Empty (WireBytes, len 0)
	p3, err := ParseFields(f3)
	if err != nil {
		t.Fatalf("Frame 3 파싱 실패: %v", err)
	}
	var commitFound bool
	for _, f := range p3 {
		if f.Num == extPrefetchInputReq {
			sub, _ := ParseFields(f.Bytes)
			for _, sf := range sub {
				if sf.Num == 2 && sf.WireType == WireBytes && len(sf.Bytes) == 0 {
					commitFound = true
				}
			}
		}
	}
	if !commitFound {
		t.Fatalf("Frame 3에 Empty commit(field 2, WireBytes) 누락")
	}

	// ProcessQueryResponse 디코딩 검증
	dt := NewBuilder()
	dt.AppendString(1, "반갑습니다!")
	thDt := NewBuilder()
	thDt.AppendString(1, "인사말 작성 중")
	th := NewBuilder()
	th.AppendMessage(1, thDt.Bytes())
	mr := NewBuilder()
	mr.AppendString(1, "rc_1")
	mr.AppendMessage(2, dt.Bytes())
	mr.AppendMessage(12, th.Bytes())
	mr.AppendInt32(19, CompletenessComplete)

	ids := NewBuilder()
	ids.AppendString(1, "c_abc")
	ids.AppendString(2, "r_xyz")

	tu := NewBuilder()
	tu.AppendString(1, "인사 대화")

	cgr := NewBuilder()
	cgr.AppendMessage(1, mr.Bytes())
	cgr.AppendMessage(2, ids.Bytes())
	cgr.AppendMessage(4, tu.Bytes())
	cgr.AppendBool(9, true)
	cgr.AppendString(30, "Gemini 3")

	stream := NewBuilder()
	stream.AppendMessage(1, cgr.Bytes())

	respFrame := NewBuilder()
	respFrame.AppendMessage(extClientGenerateRespStream, stream.Bytes())

	chunks, err := DecodeProcessQueryResponseFrame(respFrame.Bytes())
	if err != nil || len(chunks) != 1 {
		t.Fatalf("응답 디코딩 실패: chunks=%d err=%v", len(chunks), err)
	}
	c := chunks[0]
	if c.ConversationID != "c_abc" || c.RequestID != "r_xyz" || c.ResponseID != "rc_1" ||
		c.Text != "반갑습니다!" || c.Thoughts != "인사말 작성 중" || c.Title != "인사 대화" || !c.TreeCompleted {
		t.Fatalf("디코딩 결과 불일치: %+v", c)
	}
}

func TestDecodeConversationTurnRichFields(t *testing.T) {
	// 사용자 첨부 이미지 (SavedUserQuery.f5.f4)
	dim := NewBuilder()
	dim.AppendInt32(1, 485)
	dim.AppendInt32(2, 1000)

	attItem := NewBuilder()
	attItem.AppendString(3, "sample.jpg")
	attItem.AppendString(4, "https://lh3.googleusercontent.com/gg/sample")
	attItem.AppendString(12, "image/jpeg")
	attItem.AppendMessage(16, dim.Bytes())

	attList := NewBuilder()
	attList.AppendMessage(4, attItem.Bytes())

	uq := NewBuilder()
	uq.AppendString(1, "이미지 편집해줘")
	uq.AppendMessage(5, attList.Bytes())

	savedReq := NewBuilder()
	savedReq.AppendMessage(1, uq.Bytes())

	// Canvas 아티팩트 (ModelResponse.f13)
	artMeta := NewBuilder()
	artMeta.AppendString(1, "c_1_demo.tsx")
	artMeta.AppendString(13, "react")

	artCode := NewBuilder()
	artCode.AppendString(1, "export default function App() { return null; }")
	artCode.AppendString(2, "react")

	artF1 := NewBuilder()
	artF1.AppendMessage(5, artCode.Bytes())
	artF3 := NewBuilder()
	artF3.AppendMessage(1, artF1.Bytes())

	art := NewBuilder()
	art.AppendMessage(1, artMeta.Bytes())
	art.AppendMessage(3, artF3.Bytes())
	art.AppendString(6, "Demo Canvas App")
	art.AppendString(7, "https://gemini.google.com/app/1?canvas-id=demo")

	mr := NewBuilder()
	mr.AppendString(1, "rc_10")
	mr.AppendString(4, "결과입니다.\nhttp://googleusercontent.com/immersive_entry_chip/0")
	mr.AppendMessage(13, art.Bytes())

	// 예약 작업 제목 (StatusUpdateResponse.f13.f9)
	remy := NewBuilder()
	remy.AppendString(9, "일일 브리핑")
	st := NewBuilder()
	st.AppendMessage(13, remy.Bytes())

	savedResp := NewBuilder()
	savedResp.AppendMessage(1, mr.Bytes())
	savedResp.AppendString(4, "rc_10")
	savedResp.AppendMessage(5, st.Bytes())
	savedResp.AppendBool(11, true) // aborted

	ids := NewBuilder()
	ids.AppendString(1, "c_1")
	ids.AppendString(2, "r_1")

	schedFlag := NewBuilder()
	schedFlag.AppendInt32(1, 1)

	turn := NewBuilder()
	turn.AppendMessage(1, ids.Bytes())
	turn.AppendMessage(3, savedReq.Bytes())
	turn.AppendMessage(4, savedResp.Bytes())
	turn.AppendMessage(17, schedFlag.Bytes())

	resp := NewBuilder()
	resp.AppendMessage(1, turn.Bytes())

	decoded, err := DecodeListConversationTurnsResponse(resp.Bytes())
	if err != nil || len(decoded.Turns) != 1 {
		t.Fatalf("턴 디코딩 실패: turns=%d err=%v", len(decoded.Turns), err)
	}
	got := decoded.Turns[0]
	if len(got.UserAttachments) != 1 || got.UserAttachments[0].Name != "sample.jpg" ||
		got.UserAttachments[0].Width != 485 || got.UserAttachments[0].Height != 1000 {
		t.Fatalf("사용자 첨부 디코딩 불일치: %+v", got.UserAttachments)
	}
	if len(got.Artifacts) != 1 || got.Artifacts[0].Title != "Demo Canvas App" ||
		got.Artifacts[0].Language != "react" || got.Artifacts[0].Content == "" ||
		got.Artifacts[0].URL == "" {
		t.Fatalf("Canvas 아티팩트 디코딩 불일치: %+v", got.Artifacts)
	}
	if !got.IsScheduledRun || got.ScheduledTitle != "일일 브리핑" || !got.Aborted {
		t.Fatalf("예약/중단 메타 디코딩 불일치: sched=%v title=%q aborted=%v", got.IsScheduledRun, got.ScheduledTitle, got.Aborted)
	}
}

func TestCleanToolStatusTextAndDedup(t *testing.T) {
	got := cleanToolStatusText("{tool_logo_placeholder} {tool_name_placeholder}에 연결 중", "YouTube")
	if got != "YouTube에 연결 중" {
		t.Fatalf("플레이스홀더 정제 실패: %q", got)
	}

	deduped := dedupToolActivities([]RemoteToolActivity{
		{Name: "YouTube", Action: "YouTube에 연결 중"},
		{Name: "YouTube", Action: "YouTube에 연결 중"},
		{Name: "Google Search", Action: "검색 중"},
	})
	if len(deduped) != 2 {
		t.Fatalf("중복 도구 활동 제거 실패: %+v", deduped)
	}
}

func TestEncodeAndDecodeDriveAttachment(t *testing.T) {
	f1, f2, f3 := EncodeProcessQueryFrames(nil, ChatQueryParams{
		Text: "드라이브 파일 분석",
		Attachments: []ChatAttachment{
			{
				Name:            "report.pdf",
				MimeType:        "application/pdf",
				DriveResourceID: "1abcDEF_drive_id",
			},
		},
	})
	if len(f1) == 0 || len(f2) == 0 || len(f3) == 0 {
		t.Fatal("프레임 인코딩 실패")
	}

	// SavedUserAttachments (field 5: DriveFile) 디코딩 검증
	df := NewBuilder()
	df.AppendString(1, "1abcDEF_drive_id")
	df.AppendString(2, "report.pdf")
	df.AppendString(3, "application/pdf")

	att := NewBuilder()
	att.AppendMessage(5, df.Bytes())

	decoded := decodeSavedUserAttachments(att.Bytes())
	if len(decoded) != 1 {
		t.Fatalf("드라이브 첨부 디코딩 개수 불일치: %+v", decoded)
	}
	wantURL := "https://drive.google.com/file/d/1abcDEF_drive_id/view"
	if decoded[0].URL != wantURL || decoded[0].Name != "report.pdf" || decoded[0].MimeType != "application/pdf" {
		t.Fatalf("드라이브 첨부 디코딩 값 불일치: %+v", decoded[0])
	}
}

func TestDecodeModelResponseExtractsFollowUpAndElicitations(t *testing.T) {
	displayRaw := "오늘 파주는 맑고 쌀쌀합니다.\n\n" +
		`<FollowUp label="옷차림 가이드를 정리해 드릴까요?" query="파주 날씨에 맞는 옷차림 가이드를 정리해 주세요."/>` + "\n\n" +
		`<ElicitationsGroup message="추가로 살펴볼 수 있는 내용입니다:">` + "\n" +
		`  <Elicitation label="기온별 옷차림 기준표" query="기온별 추천 옷차림 기준표를 만들어 주세요."/>` + "\n" +
		`  <Elicitation label="11월 평년 기후 특징" query="파주시 11월 평년 기후 특징을 알려 주세요."/>` + "\n" +
		`</ElicitationsGroup>`
	cleanRaw := "오늘 파주는 맑고 쌀쌀합니다.\n\n옷차림 가이드를 정리해 드릴까요?\n\n추가로 살펴볼 수 있는 내용입니다:\n기온별 옷차림 기준표\n11월 평년 기후 특징"

	dt := NewBuilder()
	dt.AppendString(1, displayRaw)
	mr := NewBuilder()
	mr.AppendString(1, "rc_weather")
	mr.AppendMessage(2, dt.Bytes())
	mr.AppendString(4, cleanRaw)

	got := decodeModelResponse(mr.Bytes())
	if got.Text != "오늘 파주는 맑고 쌀쌀합니다." {
		t.Fatalf("후속 태그 분리 후 본문 불일치: %q", got.Text)
	}
	if got.FollowUp == nil || got.FollowUp.Label != "옷차림 가이드를 정리해 드릴까요?" ||
		got.FollowUp.Query != "파주 날씨에 맞는 옷차림 가이드를 정리해 주세요." {
		t.Fatalf("FollowUp 파싱 불일치: %+v", got.FollowUp)
	}
	if got.Elicitations == nil || got.Elicitations.Message != "추가로 살펴볼 수 있는 내용입니다:" ||
		len(got.Elicitations.Items) != 2 || got.Elicitations.Items[1].Label != "11월 평년 기후 특징" {
		t.Fatalf("ElicitationsGroup 파싱 불일치: %+v", got.Elicitations)
	}

	// 스트리밍 도중 미완성 꼬리 태그가 본문에 원시 XML로 노출되지 않아야 한다.
	partial := "답변 본문입니다.\n\n<ElicitationsGroup message=\"추가 탐색\">\n  <Elicitation label=\"항목"
	cleaned, fu, el, had := extractInteractiveSuggestions(partial)
	if !had || fu != nil || el != nil || cleaned != "답변 본문입니다." {
		t.Fatalf("스트리밍 미완성 태그 마스킹 실패: cleaned=%q fu=%+v el=%+v had=%v", cleaned, fu, el, had)
	}
}
