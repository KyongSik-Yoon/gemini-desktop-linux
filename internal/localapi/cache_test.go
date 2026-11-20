package localapi

import (
	"sync"
	"sync/atomic"
	"testing"
	"time"

	"github.com/sam/gemini-remote-linux/internal/proto"
)

func TestImageCacheLRUAndTTL(t *testing.T) {
	c := newImageCache(20, 2, 50*time.Millisecond)

	c.Put("a", []byte("12345678"), "image/png") // 8B
	c.Put("b", []byte("abcdefgh"), "image/jpeg") // 8B (총 16B)

	// "a"를 조회해 LRU 최상단으로 승격
	if got, ct, ok := c.Get("a"); !ok || string(got) != "12345678" || ct != "image/png" {
		t.Fatalf("캐시 히트 실패: ok=%v got=%q ct=%q", ok, got, ct)
	}

	// "c" 추가 시 maxEntries=2/maxBytes=20 초과로 가장 오래된 "b"가 퇴출되어야 함
	c.Put("c", []byte("ABCDEFGH"), "image/webp")
	if _, _, ok := c.Get("b"); ok {
		t.Fatalf("가장 오래된 항목 b가 퇴출되지 않음")
	}
	if _, _, ok := c.Get("a"); !ok {
		t.Fatalf("최근 조회 항목 a가 유지되어야 함")
	}

	// maxBytes/2(10B) 초과 항목은 캐시되지 않아야 함
	c.Put("huge", []byte("01234567890"), "image/png")
	if _, _, ok := c.Get("huge"); ok {
		t.Fatalf("단일 항목 용량 상한 초과 데이터가 캐시됨")
	}

	// TTL 만료 후 미스 반환
	time.Sleep(70 * time.Millisecond)
	if _, _, ok := c.Get("a"); ok {
		t.Fatalf("TTL 만료 항목이 여전히 히트됨")
	}
}

func TestSingleflightGroupDeduplicatesConcurrentCalls(t *testing.T) {
	var sf singleflightGroup[string]
	var calls atomic.Int32

	const workers = 12
	var wg sync.WaitGroup
	results := make([]string, workers)
	for i := 0; i < workers; i++ {
		wg.Add(1)
		go func(idx int) {
			defer wg.Done()
			val, err := sf.Do("same-key", func() (string, error) {
				calls.Add(1)
				time.Sleep(30 * time.Millisecond)
				return "shared-result", nil
			})
			if err != nil {
				t.Errorf("예상치 못한 오류: %v", err)
			}
			results[idx] = val
		}(i)
	}
	wg.Wait()

	if got := calls.Load(); got != 1 {
		t.Fatalf("동시 호출이 1회로 병합되어야 함: actual=%d", got)
	}
	for i, r := range results {
		if r != "shared-result" {
			t.Fatalf("워커 %d 결과 불일치: %q", i, r)
		}
	}
}

func TestCompactStreamChunkPrefixAndReplace(t *testing.T) {
	var prevText, prevThoughts string

	// 1) 첫 청크: 빈 문자열의 접두사 연장이므로 Delta만 전송하고 Text는 생략
	c1 := compactStreamChunk(proto.ChatStreamChunk{
		Type:     "data",
		Text:     "안녕",
		Thoughts: "생각1",
	}, &prevText, &prevThoughts)
	if c1.Text != "" || c1.Delta != "안녕" || c1.Thoughts != "" || c1.ThoughtsDelta != "생각1" {
		t.Fatalf("첫 청크 경량화 실패: %+v", c1)
	}

	// 2) 두 번째 청크: 순수 접두사 연장 -> 증분만 전송
	c2 := compactStreamChunk(proto.ChatStreamChunk{
		Type:     "data",
		Text:     "안녕하세요",
		Thoughts: "생각1 생각2",
	}, &prevText, &prevThoughts)
	if c2.Text != "" || c2.Delta != "하세요" || c2.Thoughts != "" || c2.ThoughtsDelta != " 생각2" {
		t.Fatalf("접두사 연장 청크 경량화 실패: %+v", c2)
	}

	// 3) 세 번째 청크: 비접두사 교체 -> 전체 Text 유지
	c3 := compactStreamChunk(proto.ChatStreamChunk{
		Type: "data",
		Text: "완전히 바뀐 응답",
	}, &prevText, &prevThoughts)
	if c3.Text != "완전히 바뀐 응답" || c3.Delta != "" {
		t.Fatalf("비접두사 교체 청크 처리 실패: %+v", c3)
	}
}
