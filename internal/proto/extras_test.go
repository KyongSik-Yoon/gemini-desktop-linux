package proto

import (
	"encoding/binary"
	"math"
	"testing"
)

func TestDecodeListGeminiNotebooksResponse(t *testing.T) {
	ts := NewBuilder()
	ts.AppendInt64(1, 1770000000)

	nb := NewBuilder()
	nb.AppendString(1, "notebooks/abc123")
	nb.AppendString(2, "시스템 설계 노트북")
	nb.AppendMessage(3, ts.Bytes())
	nb.AppendInt32(4, 5)

	resp := NewBuilder()
	resp.AppendMessage(1, nb.Bytes())

	list, err := DecodeListGeminiNotebooksResponse(resp.Bytes())
	if err != nil {
		t.Fatalf("디코딩 실패: %v", err)
	}
	if len(list) != 1 {
		t.Fatalf("예상 1개, 실제 %d개", len(list))
	}
	if list[0].ID != "notebooks/abc123" || list[0].Title != "시스템 설계 노트북" || list[0].SourceCount != 5 {
		t.Fatalf("디코딩 결과 불일치: %+v", list[0])
	}
}

func TestDecodeListRemySchedulesAndRecommended(t *testing.T) {
	hm := NewBuilder()
	hm.AppendInt32(1, 9)
	hm.AppendInt32(2, 30)

	daily := NewBuilder()
	daily.AppendMessage(1, hm.Bytes())

	sch := NewBuilder()
	sch.AppendString(1, "sch_remote_1")
	sch.AppendString(2, "매일 아침 요약")
	sch.AppendInt32(3, 1)
	sch.AppendMessage(4, daily.Bytes())
	sch.AppendString(9, "Asia/Seoul")
	sch.AppendString(10, "오늘 일정을 요약해 줘")
	sch.AppendBool(12, true)
	sch.AppendString(13, "c_last_1")
	sch.AppendString(25, "매일 오전 9:30에")

	resp := NewBuilder()
	resp.AppendMessage(1, sch.Bytes())

	list, err := DecodeListRemySchedulesResponse(resp.Bytes())
	if err != nil {
		t.Fatalf("예약 디코딩 실패: %v", err)
	}
	if len(list) != 1 || list[0].TimeOfDay != "09:30" || list[0].ScheduleDesc != "매일 오전 9:30에" || !list[0].Enabled {
		t.Fatalf("예약 디코딩 불일치: %+v", list)
	}

	tmpl := NewBuilder()
	tmpl.AppendString(1, "rec_1")
	tmpl.AppendString(2, "주간 기술 브리핑")

	rec := NewBuilder()
	rec.AppendMessage(1, tmpl.Bytes())
	rec.AppendString(3, "매주 월요일 주요 뉴스를 요약합니다")
	rec.AppendString(4, "매주 월요일 오전 9시에 최신 기술 뉴스를 정리해 줘")

	recResp := NewBuilder()
	recResp.AppendMessage(1, rec.Bytes())

	recs, err := DecodeListRecommendedSchedulesResponse(recResp.Bytes())
	if err != nil || len(recs) != 1 || recs[0].Title != "주간 기술 브리핑" {
		t.Fatalf("추천 예약 디코딩 불일치: %+v, err=%v", recs, err)
	}
}

func TestDecodeListGeminiAppArtifactsAndUsageInfo(t *testing.T) {
	ids := NewBuilder()
	ids.AppendString(1, "c_art_1")
	ids.AppendString(2, "r_art_1")
	ts := NewBuilder()
	ts.AppendInt64(1, 1770001234)
	payload := NewBuilder()
	payload.AppendString(1, "<html><body>Canvas 미리보기</body></html>")

	art := NewBuilder()
	art.AppendMessage(1, ids.Bytes())
	art.AppendMessage(2, ts.Bytes())
	art.AppendInt32(3, 6)
	art.AppendString(4, "대시보드 시안")
	art.AppendMessage(5, payload.Bytes())
	art.AppendString(6, "rc_1")

	artResp := NewBuilder()
	artResp.AppendMessage(1, art.Bytes())

	arts, err := DecodeListGeminiAppArtifactsResponse(artResp.Bytes(), false)
	if err != nil || len(arts) != 1 || arts[0].Kind != "document" || arts[0].Title != "대시보드 시안" {
		t.Fatalf("아티팩트 디코딩 불일치: %+v, err=%v", arts, err)
	}

	// UsageInfo float32 fixed32 포함 테스트
	resetTs := NewBuilder()
	resetTs.AppendInt64(1, 1770099999)

	detail := NewBuilder()
	detail.AppendMessage(1, resetTs.Bytes())
	detail.AppendTag(2, WireFixed32)
	var f32 [4]byte
	binary.LittleEndian.PutUint32(f32[:], math.Float32bits(0.42))
	detail.buf = append(detail.buf, f32[:]...)
	detail.AppendString(3, "현재 사용량")

	bucket := NewBuilder()
	bucket.AppendInt32(1, 1)
	bucket.AppendMessage(2, detail.Bytes())

	notice := NewBuilder()
	notice.AppendString(1, "사용량 한도는 주기적으로 초기화됩니다.")
	notice.AppendString(2, "https://support.google.com/gemini")

	uResp := NewBuilder()
	uResp.AppendMessage(3, bucket.Bytes())
	uResp.AppendMessage(4, notice.Bytes())

	info, err := DecodeGetUsageInfoResponse(uResp.Bytes())
	if err != nil || len(info.Buckets) != 1 {
		t.Fatalf("사용량 디코딩 실패: %+v, err=%v", info, err)
	}
	if info.Buckets[0].UsedRatio < 0.41 || info.Buckets[0].UsedRatio > 0.43 {
		t.Fatalf("사용률 불일치: %v", info.Buckets[0].UsedRatio)
	}
}

func TestDecodeConversationTurnCleanTextAndStatus(t *testing.T) {
	disp := NewBuilder()
	disp.AppendString(1, "[span_0](start_span)원본 텍스트[span_0](end_span)")

	mr := NewBuilder()
	mr.AppendString(1, "rc_clean")
	mr.AppendMessage(2, disp.Bytes())
	mr.AppendString(4, "정제된 깔끔한 마크다운 텍스트")

	toolMeta := NewBuilder()
	toolMeta.AppendString(3, "Google Search")
	toolMeta.AppendString(6, "웹 검색 중")
	toolWrap := NewBuilder()
	toolWrap.AppendString(1, "t1")
	toolWrap.AppendMessage(2, toolMeta.Bytes())
	status := NewBuilder()
	status.AppendMessage(2, toolWrap.Bytes())

	savedResp := NewBuilder()
	savedResp.AppendMessage(1, mr.Bytes())
	savedResp.AppendString(4, "rc_clean")
	savedResp.AppendMessage(5, status.Bytes())

	turn := NewBuilder()
	turn.AppendMessage(4, savedResp.Bytes())

	res := NewBuilder()
	res.AppendMessage(1, turn.Bytes())

	parsed, err := DecodeListConversationTurnsResponse(res.Bytes())
	if err != nil || len(parsed.Turns) != 1 {
		t.Fatalf("턴 디코딩 실패: %v", err)
	}
	got := parsed.Turns[0]
	if got.ModelText != "정제된 깔끔한 마크다운 텍스트" {
		t.Fatalf("정제 텍스트 미적용: %q", got.ModelText)
	}
	if len(got.ToolActivities) != 1 || got.ToolActivities[0].Name != "Google Search" {
		t.Fatalf("히스토리 도구 활동 파싱 실패: %+v", got.ToolActivities)
	}
}
