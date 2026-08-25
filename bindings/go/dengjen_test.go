package dengjen

// Go does not permit "import \"C\"" inside a _test.go file. The tests here
// exercise the cgo boundary by calling LoadVoice (defined in voice.go), which
// handles all necessary cgo interactions.

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
		"speaker_id_map": {"default": 0},
		"streaming": false,
		"espeak": {"voice": "en-us"},
		"inference": {"noise_scale": 0.667, "length_scale": 1.0, "noise_w": 0.8},
		"num_symbols": 8,
		"phoneme_map": {},
		"phoneme_id_map": {"^": [1], "$": [2], "_": [3], "t": [4], "ɛ": [5], "s": [6]},
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

func TestSynthConfigRoundTrip(t *testing.T) {
	v, err := LoadVoice(syntheticPiperConfigPath(t))
	if err != nil {
		t.Fatalf("LoadVoice failed: %v", err)
	}
	defer v.Close()

	cfg, err := v.PiperDefaultSynthConfig()
	if err != nil {
		t.Fatalf("PiperDefaultSynthConfig failed: %v", err)
	}
	if cfg.NoiseScale != 0.667 {
		t.Errorf("expected default NoiseScale 0.667, got %v", cfg.NoiseScale)
	}

	cfg.LengthScale = 2.0
	if err := v.SetPiperSynthConfig(cfg); err != nil {
		t.Fatalf("SetPiperSynthConfig failed: %v", err)
	}

	if err := v.SetSynthesisParameter("noise_scale", 0.5); err != nil {
		t.Fatalf("SetSynthesisParameter failed: %v", err)
	}
	value, ok, err := v.SynthesisParameter("noise_scale")
	if err != nil {
		t.Fatalf("SynthesisParameter failed: %v", err)
	}
	if !ok {
		t.Fatalf("expected SynthesisParameter to report the key was found")
	}
	if value != 0.5 {
		t.Errorf("expected noise_scale 0.5 after SetSynthesisParameter, got %v", value)
	}

	_, ok, err = v.SynthesisParameter("no_such_key")
	if err != nil {
		t.Fatalf("SynthesisParameter for an unset key returned an error: %v", err)
	}
	if ok {
		t.Errorf("expected ok=false for a key that was never set")
	}
}

func TestSpeakToFileWritesAWavFile(t *testing.T) {
	v, err := LoadVoice(syntheticPiperConfigPath(t))
	if err != nil {
		t.Fatalf("LoadVoice failed: %v", err)
	}
	defer v.Close()

	outPath := t.TempDir() + "/output.wav"
	params := SynthesisParams{Mode: SynthModeLazy, Rate: 50, Volume: 100, Pitch: 50}
	wrote, err := v.SpeakToFile("Test.", params, outPath)
	if err != nil {
		t.Fatalf("SpeakToFile failed: %v", err)
	}
	if !wrote {
		t.Fatalf("expected SpeakToFile to report success")
	}

	data, err := os.ReadFile(outPath)
	if err != nil {
		t.Fatalf("expected an output WAV file: %v", err)
	}
	// WAV file size: 44-byte RIFF/WAVE header plus audio samples (2 bytes per i16 sample).
	// The exact sample count depends on the synthesis implementation and input parameters.
	if len(data) < 44 {
		t.Errorf("expected a valid WAV file (at least 44 bytes), got %d bytes", len(data))
	}
}
