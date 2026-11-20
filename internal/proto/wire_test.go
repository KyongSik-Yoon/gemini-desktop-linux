package proto

import (
	"testing"
)

func TestStructRoundTrip(t *testing.T) {
	in := map[string]any{
		"command":   "ls -la",
		"exit_code": 0,
		"success":   true,
		"tags":      []any{"linux", "wayland"},
		"nested": map[string]any{
			"key": "val",
		},
	}
	encoded := EncodeStruct(in)
	decoded := DecodeStruct(encoded)

	if decoded["command"] != "ls -la" {
		t.Fatalf("command 불일치: %v", decoded["command"])
	}
	if decoded["exit_code"] != float64(0) {
		t.Fatalf("exit_code 불일치: %v", decoded["exit_code"])
	}
	if decoded["success"] != true {
		t.Fatalf("success 불일치: %v", decoded["success"])
	}
}

func TestExecuteRemoteToolsDecode(t *testing.T) {
	// ClientFunctionCallArgs.FunctionCall { 1: "run_command", 2: "call_123", 3: Struct{"command": "uname -a"} }
	fc := NewBuilder()
	fc.AppendString(1, "run_command")
	fc.AppendString(2, "call_123")
	fc.AppendMessage(3, EncodeStruct(map[string]any{"command": "uname -a"}))

	argsMsg := NewBuilder()
	argsMsg.AppendMessage(1, fc.Bytes())

	resp := NewBuilder()
	resp.AppendMessage(2, argsMsg.Bytes())
	resp.AppendString(5, "conv_999")

	ev, err := DecodeExecuteRemoteToolsResponse(resp.Bytes())
	if err != nil {
		t.Fatalf("디코딩 실패: %v", err)
	}
	if len(ev.FunctionCalls) != 1 {
		t.Fatalf("FunctionCalls 개수 불일치: %d", len(ev.FunctionCalls))
	}
	if ev.FunctionCalls[0].Name != "run_command" || ev.FunctionCalls[0].ID != "call_123" {
		t.Fatalf("FunctionCall 메타 불일치: %+v", ev.FunctionCalls[0])
	}
	if ev.FunctionCalls[0].Args["command"] != "uname -a" {
		t.Fatalf("FunctionCall 인자 불일치: %+v", ev.FunctionCalls[0].Args)
	}
}

func TestWakeUpPayloadDecode(t *testing.T) {
	dispatch := NewBuilder()
	dispatch.AppendString(1, "rk_abc123")
	dispatch.AppendString(2, "rt_token456")
	dispatch.AppendBool(3, true)

	wakeup := NewBuilder()
	wakeup.AppendMessage(1, dispatch.Bytes())
	wakeup.AppendString(4, "dev-uuid-1")

	ev, err := DecodeWakeUpPayload(2, "remote-tool-wakeup/1", wakeup.Bytes())
	if err != nil {
		t.Fatalf("WakeUp 디코딩 실패: %v", err)
	}
	if ev.SessionRoutingKey != "rk_abc123" || ev.RoutingToken != "rt_token456" || ev.TargetDeviceID != "dev-uuid-1" {
		t.Fatalf("WakeUp 필드 불일치: %+v", ev)
	}
}

func TestDecodeMultiWatchResponseBatch(t *testing.T) {
	part1 := NewBuilder()
	part1.AppendString(1, "remote-tool-wakeup")
	part2 := NewBuilder()
	part2.AppendString(1, "evt-1")

	objPath := NewBuilder()
	objPath.AppendMessage(1, part1.Bytes())
	objPath.AppendMessage(1, part2.Bytes())

	objChange := NewBuilder()
	objChange.AppendInt64(1, 42)
	objChange.AppendBytes(2, []byte("payload-bytes"))
	objChange.AppendMessage(6, objPath.Bytes())

	changeBatch := NewBuilder()
	changeBatch.AppendMessage(1, objChange.Bytes())

	watchResp := NewBuilder()
	watchResp.AppendMessage(2, changeBatch.Bytes())

	multiResp := NewBuilder()
	multiResp.AppendInt64Always(1, 2)
	multiResp.AppendMessage(2, watchResp.Bytes())

	batch := NewBuilder()
	batch.AppendMessage(1, multiResp.Bytes())

	changes, needsReconnect, err := DecodeMultiWatchResponseBatch(batch.Bytes())
	if err != nil || needsReconnect {
		t.Fatalf("DecodeMultiWatchResponseBatch 실패: %v (reconnect=%v)", err, needsReconnect)
	}
	if len(changes) != 1 {
		t.Fatalf("ObjectChange 개수 불일치: %d", len(changes))
	}
	if changes[0].StreamID != 2 || changes[0].ObjectPath != "remote-tool-wakeup/evt-1" || string(changes[0].Payload) != "payload-bytes" {
		t.Fatalf("ObjectChange 필드 불일치: %+v", changes[0])
	}
}

func TestDecodeMultiWatchResponseBatchWithActivity(t *testing.T) {
	// Case 1: Checkpoint (WatchResponse field 3)만 포함된 배치 -> hasWatchActivity=true, needsReconnect=false
	cpWatchResp := NewBuilder()
	cpWatchResp.AppendBytes(3, []byte{0x08, 0x01})
	cpMultiResp := NewBuilder()
	cpMultiResp.AppendInt64Always(1, 1)
	cpMultiResp.AppendMessage(2, cpWatchResp.Bytes())
	cpBatch := NewBuilder()
	cpBatch.AppendMessage(1, cpMultiResp.Bytes())

	changes, hasActivity, needsReconnect, err := DecodeMultiWatchResponseBatchWithActivity(cpBatch.Bytes())
	if err != nil {
		t.Fatalf("Checkpoint 디코딩 실패: %v", err)
	}
	if !hasActivity || needsReconnect || len(changes) != 0 {
		t.Fatalf("Checkpoint 판정 불일치: activity=%v, reconnect=%v, len=%d", hasActivity, needsReconnect, len(changes))
	}

	// Case 2: WatchError (WatchResponse field 4, stream=2)만 포함된 배치 -> hasWatchActivity=false, needsReconnect=true
	errStatus := NewBuilder()
	errStatus.AppendInt32Always(1, 13)
	errStatus.AppendString(2, "internal error")
	errWatchResp := NewBuilder()
	errWatchResp.AppendMessage(4, errStatus.Bytes())
	errMultiResp := NewBuilder()
	errMultiResp.AppendInt64Always(1, 2)
	errMultiResp.AppendMessage(2, errWatchResp.Bytes())
	errBatch := NewBuilder()
	errBatch.AppendMessage(1, errMultiResp.Bytes())

	changes, hasActivity, needsReconnect, err = DecodeMultiWatchResponseBatchWithActivity(errBatch.Bytes())
	if err != nil {
		t.Fatalf("WatchError 디코딩 실패: %v", err)
	}
	if hasActivity || !needsReconnect || len(changes) != 0 {
		t.Fatalf("WatchError 판정 불일치: activity=%v, reconnect=%v, len=%d", hasActivity, needsReconnect, len(changes))
	}
}

func TestParseFieldsOverflowProtection(t *testing.T) {
	// tag=0x0a (field 1, WireBytes), varint=0xffffffffffffffff (math.MaxUint64)
	malformed := []byte{0x0a, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0x01}
	if _, err := ParseFields(malformed); err == nil {
		t.Fatalf("오버플로 WireBytes 입력에서 에러가 반환되어야 함")
	}
}

func TestWakeUpRefreshDeviceStatusAndUIState(t *testing.T) {
	// ClientAction { 27: RefreshDeviceStatus { 1: "DEV-1", 3: true } }
	refresh := NewBuilder()
	refresh.AppendString(1, "DEV-1")
	refresh.AppendBool(3, true)
	action := NewBuilder()
	action.AppendMessage(27, refresh.Bytes())
	sig := NewBuilder()
	sig.AppendMessage(1, action.Bytes())

	ev, err := DecodeWakeUpPayload(1, "bard-client-sync", sig.Bytes())
	if err != nil {
		t.Fatalf("RefreshDeviceStatus 디코딩 실패: %v", err)
	}
	if !ev.RefreshDeviceStatus || ev.RefreshDeviceID != "DEV-1" || !ev.RefreshRemoteEnabled {
		t.Fatalf("RefreshDeviceStatus 필드 불일치: %+v", ev)
	}

	// FunctionResponseInput UIState(field 5) 인코딩 검증
	encoded := EncodeRemoteToolResultRequest(nil, []FunctionResponseInput{{
		Name:     "ui_control",
		ID:       "call-1",
		Response: map[string]any{"ok": true},
		UIState:  `{"focused":"editor"}`,
	}})
	top, err := ParseFields(encoded)
	if err != nil || len(top) == 0 {
		t.Fatalf("EncodeRemoteToolResultRequest 파싱 실패: %v", err)
	}
	resFields, _ := ParseFields(top[0].Bytes)
	frFields, _ := ParseFields(resFields[0].Bytes)
	var gotUIState string
	for _, f := range frFields {
		if f.Num == 5 && f.WireType == WireBytes {
			gotUIState = string(f.Bytes)
		}
	}
	if gotUIState != `{"focused":"editor"}` {
		t.Fatalf("UIState(field 5) 불일치: %q", gotUIState)
	}
}

func TestDecodeUsageInfoNaNProtected(t *testing.T) {
	// UsageInfo { 3: UsageBucket { 1: 1, 2: { 2: fixed32(NaN) } } }
	detail := NewBuilder()
	detail.AppendFixed32(2, 0x7fc00000) // quiet NaN
	bucket := NewBuilder()
	bucket.AppendInt32(1, 1)
	bucket.AppendMessage(2, detail.Bytes())
	root := NewBuilder()
	root.AppendMessage(3, bucket.Bytes())

	info, err := DecodeGetUsageInfoResponse(root.Bytes())
	if err != nil || len(info.Buckets) != 1 {
		t.Fatalf("DecodeGetUsageInfoResponse 실패: %v", err)
	}
	if info.Buckets[0].UsedRatio != 0 {
		t.Fatalf("NaN UsedRatio는 0으로 정규화되어야 함: %v", info.Buckets[0].UsedRatio)
	}
}
