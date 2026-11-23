package proto

import (
	"encoding/hex"
	"strings"
	"testing"
)

func TestCollectUnknownFieldsAndFormat(t *testing.T) {
	b := NewBuilder()
	b.AppendInt32(1, 1)                                // known
	b.AppendMessage(extProcessQueryInitialResp, nil)   // known (1000)
	b.AppendString(1042, "future-extension-payload")   // unknown bytes
	b.AppendInt32(99, 7)                               // unknown varint
	b.AppendString(1042, "duplicate-ignored-for-list") // duplicate tag

	fields, err := ParseFields(b.Bytes())
	if err != nil {
		t.Fatalf("ParseFields 실패: %v", err)
	}
	unk := CollectUnknownFields(fields, knownProcessQueryRespFields)
	if len(unk) != 2 {
		t.Fatalf("미지 필드 2건 기대, 실제=%d (%+v)", len(unk), unk)
	}
	if unk[0].Num != 99 || unk[0].WireType != WireVarint {
		t.Fatalf("첫 번째 미지 필드 불일치: %+v", unk[0])
	}
	if unk[1].Num != 1042 || unk[1].WireType != WireBytes || unk[1].Size != len("future-extension-payload") {
		t.Fatalf("두 번째 미지 필드 불일치: %+v", unk[1])
	}
	formatted := FormatUnknownFields(unk)
	if formatted != "#99(varint), #1042(bytes,24B)" {
		t.Fatalf("포맷 불일치: %q", formatted)
	}
}

// TestGoldenProcessQueryResponseWithFutureExtension은 향후 서버가 미지 확장 번호를 함께 내려도
// 기존 스트림 청크 파싱이 회귀 없이 안정적으로 동작함을 골든 와이어 픽스처로 검증한다.
func TestGoldenProcessQueryResponseWithFutureExtension(t *testing.T) {
	// 골든 와이어 조립:
	// - turn_sequence_number(1) = 1
	// - extContinuationIdentifiersRs(1014) -> { 1: "c_golden123", 2: "r_golden456" }
	// - extClientGenerateRespStream(1003) -> { 1: ClientGenerateResponse(text="안녕", model="Gemini 3"), 2: end_of_data }
	// - 미지 미래 확장 1099 -> "v2-meta"
	idInner := NewBuilder()
	idInner.AppendString(1, "c_golden123")
	idInner.AppendString(2, "r_golden456")
	idWrap := NewBuilder()
	idWrap.AppendMessage(1, idInner.Bytes())

	disp := NewBuilder()
	disp.AppendString(1, "안녕")
	mr := NewBuilder()
	mr.AppendString(1, "rc_golden789")
	mr.AppendMessage(2, disp.Bytes())
	mr.AppendInt32(19, CompletenessComplete)

	genResp := NewBuilder()
	genResp.AppendMessage(1, mr.Bytes())
	genResp.AppendBool(9, true)
	genResp.AppendString(30, "Gemini 3")
	genResp.AppendString(77, "future-resp-field")

	streamMsg := NewBuilder()
	streamMsg.AppendMessage(1, genResp.Bytes())
	streamMsg.AppendMessage(2, nil)

	frame := NewBuilder()
	frame.AppendInt32(1, 1)
	frame.AppendMessage(extContinuationIdentifiersRs, idWrap.Bytes())
	frame.AppendMessage(extClientGenerateRespStream, streamMsg.Bytes())
	frame.AppendString(1099, "v2-meta")

	// 골든 hex 왕복 검증 (비공개 정보 없음)
	goldenHex := hex.EncodeToString(frame.Bytes())
	raw, err := hex.DecodeString(goldenHex)
	if err != nil {
		t.Fatalf("골든 hex 디코딩 실패: %v", err)
	}

	chunks, err := DecodeProcessQueryResponseFrame(raw)
	if err != nil {
		t.Fatalf("DecodeProcessQueryResponseFrame 실패: %v", err)
	}
	if len(chunks) < 3 {
		t.Fatalf("청크 수 부족: %+v", chunks)
	}
	if chunks[0].ConversationID != "c_golden123" || chunks[0].RequestID != "r_golden456" {
		t.Fatalf("식별자 청크 불일치: %+v", chunks[0])
	}
	if chunks[1].Text != "안녕" || chunks[1].ModelAttribution != "Gemini 3" || chunks[1].ResponseID != "rc_golden789" {
		t.Fatalf("본문 청크 불일치: %+v", chunks[1])
	}
	if !chunks[2].TreeCompleted {
		t.Fatalf("end_of_data 청크 누락: %+v", chunks[2])
	}

	// 미지 확장 1099가 CollectUnknownFields로 정확히 감지되는지 확인
	topFields, _ := ParseFields(raw)
	unk := CollectUnknownFields(topFields, knownProcessQueryRespFields)
	if len(unk) != 1 || unk[0].Num != 1099 {
		t.Fatalf("미지 확장 1099 감지 실패: %+v", unk)
	}
	if !strings.Contains(FormatUnknownFields(unk), "#1099(bytes,7B)") {
		t.Fatalf("요약 포맷 오류: %s", FormatUnknownFields(unk))
	}
}
