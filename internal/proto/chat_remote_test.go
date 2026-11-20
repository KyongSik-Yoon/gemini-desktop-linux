package proto

import "testing"

func TestEncodeProcessQueryFrames_AgentTaskAndRemoteDeviceTarget(t *testing.T) {
	meta := EncodeChatRequestMetadata("ko-KR", "Asia/Seoul", "11111111-2222-4333-8444-5555555555aa")
	f1, _, _ := EncodeProcessQueryFrames(meta, ChatQueryParams{
		Text:                "hostname 확인해줘",
		AgentTask:           true,
		TargetDeviceID:      "11111111-2222-4333-8444-5555555555aa",
		TargetDeviceName:    "test-linux-host",
		TargetDeliveryToken: "tok-123",
	})

	top, err := ParseFields(f1)
	if err != nil {
		t.Fatalf("f1 파싱 실패: %v", err)
	}
	var initBytes []byte
	for _, f := range top {
		if f.Num == extProcessQueryInitialReq {
			initBytes = f.Bytes
		}
	}
	if len(initBytes) == 0 {
		t.Fatal("ProcessQueryInitialRequest가 비어 있습니다")
	}

	initFields, _ := ParseFields(initBytes)
	var genFeatBytes []byte
	for _, f := range initFields {
		if f.Num == extClientGenerateFeature {
			genFeatBytes = f.Bytes
		}
	}
	genFeatFields, _ := ParseFields(genFeatBytes)
	var genReqBytes []byte
	for _, f := range genFeatFields {
		if f.Num == 1 {
			genReqBytes = f.Bytes
		}
	}

	genReqFields, _ := ParseFields(genReqBytes)
	var sawAgentChat bool
	var featureMode uint64
	var targetBytes []byte
	for _, f := range genReqFields {
		if f.Num == 26 && f.WireType == WireVarint {
			featureMode = f.Varint
		}
		if f.Num == 49 && f.Varint == 1 {
			sawAgentChat = true
		}
		if f.Num == 51 && f.WireType == WireBytes {
			targetBytes = f.Bytes
		}
	}
	if featureMode != uint64(FeatureModeJanusRemy) {
		t.Fatalf("ClientGenerateRequest field 26(user_selected_feature_mode) 불일치: got=%d want=%d", featureMode, FeatureModeJanusRemy)
	}
	if !sawAgentChat {
		t.Fatal("ClientGenerateRequest field 49(is_agent_chat)가 설정되지 않았습니다")
	}
	if len(targetBytes) == 0 {
		t.Fatal("ClientGenerateRequest field 51(remote_device_target)가 비어 있습니다")
	}

	targetFields, _ := ParseFields(targetBytes)
	var gotID, gotName, gotToken string
	var gotType uint64
	for _, tf := range targetFields {
		switch tf.Num {
		case 1:
			gotID = string(tf.Bytes)
		case 2:
			gotName = string(tf.Bytes)
		case 4:
			gotToken = string(tf.Bytes)
		case 7:
			gotType = tf.Varint
		}
	}
	if gotID != "11111111-2222-4333-8444-5555555555AA" {
		t.Fatalf("device_id 불일치: %q", gotID)
	}
	if gotName != "test-linux-host" || gotToken != "tok-123" || gotType != uint64(RemoteDeviceTypeMacOS) {
		t.Fatalf("remote_device_target 필드 불일치: name=%q token=%q type=%d", gotName, gotToken, gotType)
	}
}

func TestDecodeClientGenerateResponse_StatusAndRemoteDeviceTarget(t *testing.T) {
	toolCall := NewBuilder()
	toolCall.AppendString(1, "호스트 이름을 확인합니다")
	toolCall.AppendString(2, "hostname 실행")
	toolCall.AppendString(3, "run_command")
	toolCall.AppendString(5, "run_command")
	toolCall.AppendString(10, "tc_1")

	remy := NewBuilder()
	remy.AppendMessage(1, toolCall.Bytes())
	remy.AppendString(9, "호스트 확인")

	statusUpd := NewBuilder()
	statusUpd.AppendString(1, "원격 PC에서 명령 실행 중")
	statusUpd.AppendMessage(13, remy.Bytes())

	savedTarget := NewBuilder()
	savedTarget.AppendString(1, "11111111-2222-4333-8444-5555555555AA")
	savedTarget.AppendString(2, "test-linux-host")
	savedTarget.AppendInt32(3, RemoteDeviceTypeMacOS)

	genResp := NewBuilder()
	genResp.AppendMessage(6, statusUpd.Bytes())
	genResp.AppendMessage(34, savedTarget.Bytes())

	ch := decodeClientGenerateResponse(genResp.Bytes())
	if ch.StatusText != "원격 PC에서 명령 실행 중" {
		t.Fatalf("StatusText 불일치: %q", ch.StatusText)
	}
	if ch.TargetDeviceName != "test-linux-host" {
		t.Fatalf("TargetDeviceName 불일치: %q", ch.TargetDeviceName)
	}
	if len(ch.ToolActivities) != 1 || ch.ToolActivities[0].Name != "run_command" || ch.ToolActivities[0].Summary != "hostname 실행" {
		t.Fatalf("ToolActivities 불일치: %+v", ch.ToolActivities)
	}
}
