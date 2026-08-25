package dengjen

// Go does not permit "import \"C\"" inside a _test.go file. The tests here
// exercise the cgo boundary by calling LoadVoice (defined in voice.go), which
// handles all necessary cgo interactions.

import (
	"os"
	"runtime"
	"testing"
	"time"
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
	params := SynthesisParams{Mode: SynthModeLazy, Rate: 10, Volume: 100, Pitch: 50}
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
	// BATCH_OUTPUT_SAMPLES=8000 in generate_synthetic_piper.py: a 44-byte
	// RIFF/WAVE header plus 8000 mono i16 samples. Matches the CLI's own
	// equivalent assertion in crates/frontends/cli/tests/piper_synthetic_cli.rs.
	const expectedLen = 44 + 8000*2
	if len(data) != expectedLen {
		t.Errorf("expected a %d-byte WAV file, got %d bytes", expectedLen, len(data))
	}
}

func TestSpeakLazyModeDeliversSpeechThenFinished(t *testing.T) {
	v, err := LoadVoice(syntheticPiperConfigPath(t))
	if err != nil {
		t.Fatalf("LoadVoice failed: %v", err)
	}
	defer v.Close()

	var events []EventType
	var totalBytes int
	params := SynthesisParams{Mode: SynthModeLazy, Rate: 10, Volume: 100, Pitch: 50}
	err = v.Speak("Test.", params, func(e SynthesisEvent) bool {
		events = append(events, e.Type)
		totalBytes += len(e.Data)
		if e.Err != nil {
			t.Errorf("unexpected event error: %v", e.Err)
		}
		return true
	})
	if err != nil {
		t.Fatalf("Speak failed: %v", err)
	}
	if len(events) == 0 || events[len(events)-1] != EventFinished {
		t.Fatalf("expected the last event to be EventFinished, got %v", events)
	}
	if totalBytes != 8000*2 {
		t.Errorf("expected 16000 total PCM bytes (8000 i16 samples), got %d", totalBytes)
	}
}

func TestSpeakOnEventReturningFalseStopsEarly(t *testing.T) {
	v, err := LoadVoice(syntheticPiperConfigPath(t))
	if err != nil {
		t.Fatalf("LoadVoice failed: %v", err)
	}
	defer v.Close()

	calls := 0
	params := SynthesisParams{Mode: SynthModeLazy, Rate: 10, Volume: 100, Pitch: 50}
	err = v.Speak("Test.", params, func(e SynthesisEvent) bool {
		calls++
		return false // stop immediately
	})
	if err != nil {
		t.Fatalf("Speak failed: %v", err)
	}
	if calls != 1 {
		t.Errorf("expected exactly 1 callback invocation after returning false, got %d", calls)
	}
}

// TestSpeakOnEventReturningFalseReleasesHandle proves the cgo.Handle backing
// onEvent is actually deleted when onEvent stops the stream early, not just
// that the callback stopped firing. iterate_stream (Rust side) never
// delivers a terminal (Finished/Error) event on this path, so a trampoline
// that deletes the handle only on a terminal event leaks it (and everything
// onEvent captures) permanently -- calls==1 alone can't distinguish "handle
// deleted, stream stopped cleanly" from "handle leaked forever". This test
// closes over a canary value with a finalizer: if the handle were leaked,
// the runtime's internal handle table would keep the canary reachable
// forever and its finalizer would never run.
func TestSpeakOnEventReturningFalseReleasesHandle(t *testing.T) {
	v, err := LoadVoice(syntheticPiperConfigPath(t))
	if err != nil {
		t.Fatalf("LoadVoice failed: %v", err)
	}
	defer v.Close()

	// Deliberately non-zero-size: a zero-size *canary would alias the
	// runtime's shared zerobase address with every other zero-size
	// allocation, and per runtime.SetFinalizer's documented caveat, a
	// finalizer set on a zero-size object is never guaranteed to run --
	// that would make this test pass regardless of whether Speak's handle
	// was actually released, defeating its purpose.
	type canary struct{ n int }

	released := make(chan struct{})
	// Build the onEvent closure inside its own function scope so the canary
	// it captures isn't also reachable from a variable in this test's frame
	// (which would mask a real leak: the handle table isn't the only thing
	// keeping the canary alive).
	newOnEvent := func() func(SynthesisEvent) bool {
		c := &canary{n: 1}
		runtime.SetFinalizer(c, func(*canary) { close(released) })
		return func(e SynthesisEvent) bool {
			runtime.KeepAlive(c)
			return false // stop immediately, like the early-stop test above
		}
	}

	params := SynthesisParams{Mode: SynthModeLazy, Rate: 10, Volume: 100, Pitch: 50}
	if err := v.Speak("Test.", params, newOnEvent()); err != nil {
		t.Fatalf("Speak failed: %v", err)
	}

	deadline := time.Now().Add(5 * time.Second)
	for time.Now().Before(deadline) {
		runtime.GC()
		select {
		case <-released:
			return // handle (and the closure/canary it pinned) was released
		case <-time.After(50 * time.Millisecond):
		}
	}
	t.Fatal("canary finalizer never ran: the cgo.Handle for an early-stopped " +
		"Speak call was never deleted, leaking onEvent's closure")
}
