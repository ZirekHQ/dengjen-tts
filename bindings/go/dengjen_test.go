package dengjen

// Go does not permit "import \"C\"" inside a _test.go file, so this test
// exercises the cgo boundary through smokeTestLoadVoiceFromConfigPath (see
// error.go), a thin unexported wrapper around the raw C call.

import "testing"

func TestLoadVoiceReportsAnErrorForAMissingConfigPath(t *testing.T) {
	ptr, err := smokeTestLoadVoiceFromConfigPath("/nonexistent-dengjen-go-binding-test.json")
	if ptr != nil {
		t.Fatalf("expected a null voice pointer for a missing config path, got non-null")
	}
	if err == nil {
		t.Fatalf("expected an error for a missing config path, got nil")
	}
	t.Logf("got expected error: %v", err)
}
