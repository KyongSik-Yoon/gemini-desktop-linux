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
