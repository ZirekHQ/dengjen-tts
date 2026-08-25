package dengjen

// Go does not permit "import \"C\"" inside a _test.go file, so this test
// exercises the cgo boundary through smokeTestLoadVoiceFromConfigPath (see
// error.go), a thin unexported wrapper around the raw C call.

import (
	"os"
	"testing"
)

func TestLoadVoiceReportsAnErrorForAMissingConfigPath(t *testing.T) {
	v, err := LoadVoice("/nonexistent-dengjen-go-binding-test.json")
	if v != nil {
		t.Fatalf("expected a nil voice for a missing config path, got non-nil")
	}
	if err == nil {
		t.Fatalf("expected an error for a missing config path, got nil")
	}
	t.Logf("got expected error: %v", err)
}

func syntheticPiperConfigPath(t *testing.T) string {
	t.Helper()
	dir := t.TempDir()
	modelSrc := "../../crates/dengjen/models/piper/tests/fixtures/synthetic_piper_batch.onnx"
	modelDst := dir + "/model.onnx"
	data, err := os.ReadFile(modelSrc)
	if err != nil {
		t.Fatalf("failed to read fixture %s: %v", modelSrc, err)
	}
	if err := os.WriteFile(modelDst, data, 0o644); err != nil {
		t.Fatalf("failed to copy fixture: %v", err)
	}
	configJSON := `{
		"key": null,
		"language": {"code": "en-US"},
		"audio": {"sample_rate": 22050, "quality": null},
		"num_speakers": 1,
		"speaker_id_map": {},
		"streaming": false,
		"espeak": {"voice": "en-us"},
		"inference": {"noise_scale": 0.667, "length_scale": 1.0, "noise_w": 0.8},
		"num_symbols": 8,
		"phoneme_map": {},
		"phoneme_id_map": {"^": [1], "$": [2], "_": [3], "t": [4]},
		"phoneme_type": "text",
		"hop_length": 256
	}`
	configPath := dir + "/model.onnx.json"
	if err := os.WriteFile(configPath, []byte(configJSON), 0o644); err != nil {
		t.Fatalf("failed to write config: %v", err)
	}
	return configPath
}

func TestLoadVoiceAudioInfoAndClose(t *testing.T) {
	v, err := LoadVoice(syntheticPiperConfigPath(t))
	if err != nil {
		t.Fatalf("LoadVoice failed: %v", err)
	}
	info, err := v.AudioInfo()
	if err != nil {
		t.Fatalf("AudioInfo failed: %v", err)
	}
	if info.SampleRate != 22050 {
		t.Errorf("expected SampleRate 22050, got %d", info.SampleRate)
	}
	if info.NumChannels != 1 {
		t.Errorf("expected NumChannels 1, got %d", info.NumChannels)
	}
	if err := v.Close(); err != nil {
		t.Fatalf("Close failed: %v", err)
	}
	// Close must be idempotent.
	if err := v.Close(); err != nil {
		t.Fatalf("second Close failed: %v", err)
	}
}
