package dengjen

// Go does not permit "import \"C\"" inside a _test.go file. The tests here
// exercise the cgo boundary by calling LoadVoice (defined in voice.go), which
// handles all necessary cgo interactions.

import (
	"os"
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

// TestSpeakOnEventReturningFalseReleasesCallbackData proves the callbackData
// backing onEvent is actually unpinned when onEvent stops the stream early,
// not just that the callback stopped firing. iterate_stream (Rust side)
// never delivers a terminal (Finished/Error) event on this path, so a
// trampoline that unpins only on a terminal event leaks it (and everything
// onEvent captures) permanently -- calls==1 alone can't distinguish
// "unpinned, stream stopped cleanly" from "leaked forever".
//
// An earlier version of this test tried to observe the leak indirectly via
// a GC finalizer on a canary value captured by onEvent's closure. That
// approach turned out to be unreliable in practice: run repeatedly in the
// same process (go test -count=N), it failed on every other run even with a
// 90-second timeout, while an equivalent finalizer test using a pinned
// struct alone (no Speak, no native library call) passed 10/10 instantly.
// The difference traces to the native synthesis library's own background
// threads, which apparently affect how promptly Go's GC can prove the
// closure unreachable -- unrelated to whether Unpin() actually ran. Rather
// than infer Unpin() indirectly through the garbage collector, this test
// observes it directly via testCallbackDataReleased (callback.go), a hook
// invoked synchronously at the same place Unpin() is called. That makes the
// test deterministic: no GC, no timeout, no possibility of native-library
// thread scheduling causing a false failure.
func TestSpeakOnEventReturningFalseReleasesCallbackData(t *testing.T) {
	v, err := LoadVoice(syntheticPiperConfigPath(t))
	if err != nil {
		t.Fatalf("LoadVoice failed: %v", err)
	}
	defer v.Close()

	released := false
	prevHook := testCallbackDataReleased
	testCallbackDataReleased = func() { released = true }
	defer func() { testCallbackDataReleased = prevHook }()

	params := SynthesisParams{Mode: SynthModeLazy, Rate: 10, Volume: 100, Pitch: 50}
	err = v.Speak("Test.", params, func(e SynthesisEvent) bool {
		return false // stop immediately, like the early-stop test above
	})
	if err != nil {
		t.Fatalf("Speak failed: %v", err)
	}

	// SynthModeLazy makes libdengjenSpeak (and so C.libdengjenSpeak, and so
	// Speak itself) block until the stream ends, so by the time Speak has
	// returned above, the trampoline -- and any Unpin()+hook call it was
	// going to make -- has already run synchronously on this goroutine; no
	// wait is needed here.
	if !released {
		t.Fatal("the callbackData for an early-stopped Speak call was never " +
			"released (testCallbackDataReleased hook never fired), leaking onEvent's closure")
	}
}

func TestCancelOnAVoiceWithNoActiveRealtimeStreamIsANoop(t *testing.T) {
	v, err := LoadVoice(syntheticPiperConfigPath(t))
	if err != nil {
		t.Fatalf("LoadVoice failed: %v", err)
	}
	defer v.Close()

	// No realtime-mode Speak call is in flight; Cancel must not error or panic.
	if err := v.Cancel(); err != nil {
		t.Fatalf("Cancel on an idle voice failed: %v", err)
	}
}

func TestSpeakRealtimeModeThenCancelDoesNotPanicOrDeadlock(t *testing.T) {
	v, err := LoadVoice(syntheticPiperConfigPath(t))
	if err != nil {
		t.Fatalf("LoadVoice failed: %v", err)
	}
	defer v.Close()

	// Buffered so the callback (invoked from the Rust thread pool) never blocks on this channel.
	events := make(chan SynthesisEvent, 64)
	done := make(chan error, 1)
	params := SynthesisParams{Mode: SynthModeRealtime, Rate: 10, Volume: 100, Pitch: 50, Nonblocking: true}
	err = v.Speak("Test.", params, func(e SynthesisEvent) bool {
		events <- e
		return true
	})
	if err != nil {
		t.Fatalf("Speak (nonblocking realtime) failed: %v", err)
	}
	go func() { done <- v.Cancel() }()

	select {
	case err := <-done:
		if err != nil {
			t.Fatalf("Cancel failed: %v", err)
		}
	case <-time.After(5 * time.Second):
		t.Fatal("Cancel did not return within 5s — possible deadlock")
	}

	// Cancel succeeding only proves the FFI call returned, not that the stream it interrupted
	// ever delivered its terminal event -- wait for that explicitly so this test actually
	// exercises the nonblocking event-delivery path instead of degrading into a no-op.
	deadline := time.After(5 * time.Second)
	for {
		select {
		case e := <-events:
			if e.Type == EventFinished || e.Type == EventError {
				return
			}
		case <-deadline:
			t.Fatal("no terminal event delivered within 5s after Cancel")
		}
	}
}
