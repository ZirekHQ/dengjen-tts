CARGO_MANIFEST := ../../Cargo.toml
TARGET_DIR := ../../target/release

.PHONY: native test clean

native:
	cargo build --release --manifest-path $(CARGO_MANIFEST) -p libdengjen

test: native
	LD_LIBRARY_PATH=$(TARGET_DIR):$$LD_LIBRARY_PATH \
	DYLD_LIBRARY_PATH=$(TARGET_DIR):$$DYLD_LIBRARY_PATH \
	go test ./... -v

clean:
	go clean
