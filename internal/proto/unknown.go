package proto

import (
	"fmt"
	"os"
	"sort"
	"strings"
)

// UnknownField는 현재 디코더가 명시적으로 처리하지 않는 Protobuf 태그 요약 정보다.
// 원시 페이로드 바이트나 민감 텍스트는 담지 않고 필드 번호·와이어 타입·크기만 기록한다.
type UnknownField struct {
	Num      uint32
	WireType uint8
	Size     int
}

func wireTypeName(wt uint8) string {
	switch wt {
	case WireVarint:
		return "varint"
	case WireFixed64:
		return "fixed64"
	case WireBytes:
		return "bytes"
	case WireFixed32:
		return "fixed32"
	default:
		return fmt.Sprintf("wt%d", wt)
	}
}

// CollectUnknownFields는 파싱된 필드 목록 중 known 맵에 없는 필드 번호를 정렬해 반환한다.
func CollectUnknownFields(fields []Field, known map[uint32]bool) []UnknownField {
	if len(fields) == 0 {
		return nil
	}
	var out []UnknownField
	seen := make(map[uint64]bool)
	for _, f := range fields {
		if known[f.Num] {
			continue
		}
		key := (uint64(f.Num) << 3) | uint64(f.WireType)
		if seen[key] {
			continue
		}
		seen[key] = true
		sz := 0
		if f.WireType == WireBytes {
			sz = len(f.Bytes)
		}
		out = append(out, UnknownField{
			Num:      f.Num,
			WireType: f.WireType,
			Size:     sz,
		})
	}
	sort.Slice(out, func(i, j int) bool {
		if out[i].Num != out[j].Num {
			return out[i].Num < out[j].Num
		}
		return out[i].WireType < out[j].WireType
	})
	return out
}

// FormatUnknownFields는 미처리 필드 목록을 간결한 1줄 문자열로 직렬화한다.
func FormatUnknownFields(unknown []UnknownField) string {
	if len(unknown) == 0 {
		return ""
	}
	parts := make([]string, 0, len(unknown))
	for _, u := range unknown {
		if u.WireType == WireBytes {
			parts = append(parts, fmt.Sprintf("#%d(%s,%dB)", u.Num, wireTypeName(u.WireType), u.Size))
		} else {
			parts = append(parts, fmt.Sprintf("#%d(%s)", u.Num, wireTypeName(u.WireType)))
		}
	}
	return strings.Join(parts, ", ")
}

func debugWireEnabled() bool {
	return os.Getenv("GEMINI_DEBUG_WIRE") == "1"
}

// logUnknownFieldsIfDebug는 GEMINI_DEBUG_WIRE=1 환경에서만 미처리 프로토콜 필드를 표준 오류로 요약 출력한다.
func logUnknownFieldsIfDebug(scope string, fields []Field, known map[uint32]bool) {
	if !debugWireEnabled() {
		return
	}
	unk := CollectUnknownFields(fields, known)
	if len(unk) == 0 {
		return
	}
	fmt.Fprintf(os.Stderr, "[proto][unknown] %s 미처리 필드 감지: %s\n", scope, FormatUnknownFields(unk))
}

var (
	knownProcessQueryRespFields = map[uint32]bool{
		1:                            true, // turn_sequence_number
		extProcessQueryInitialResp:   true, // 1000
		extClientGenerateRespStream:  true, // 1003
		extHalfCloseResp:             true, // 1009
		extContinuationIdentifiersRs: true, // 1014
	}
	knownClientGenerateRespFields = map[uint32]bool{
		1:  true, // repeated ModelResponse
		2:  true, // ResponseIdentifiers
		4:  true, // TitleUpdate
		9:  true, // is_message_tree_completed
		30: true, // model_attribution_name
	}
	knownClientSyncActionFields = map[uint32]bool{
		1:  true, // RefreshConversation
		21: true, // RefreshConversationsMetadata
		25: true, // WakeUpForRemoteTool
		27: true, // RefreshDeviceStatus
	}
)
