CARGO_MANIFEST := ../../Cargo.toml
TARGET_DIR := ../../target/release

.PHONY: native test test-race clean

native:
	cargo build --release --manifest-path $(CARGO_MANIFEST) -p libdengjen

test: native
	LD_LIBRARY_PATH=$(TARGET_DIR):$$LD_LIBRARY_PATH \
	DYLD_LIBRARY_PATH=$(TARGET_DIR):$$DYLD_LIBRARY_PATH \
	go test ./... -v

test-race: native
	LD_LIBRARY_PATH=$(TARGET_DIR):$$LD_LIBRARY_PATH \
	DYLD_LIBRARY_PATH=$(TARGET_DIR):$$DYLD_LIBRARY_PATH \
	go test -race ./... -v

clean:
	go clean
