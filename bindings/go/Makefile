CARGO_MANIFEST := ../../Cargo.toml
TARGET_DIR := ../../target/release

.PHONY: native test test-race vet fmt-check clean

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

vet: native
	LD_LIBRARY_PATH=$(TARGET_DIR):$$LD_LIBRARY_PATH \
	DYLD_LIBRARY_PATH=$(TARGET_DIR):$$DYLD_LIBRARY_PATH \
	go vet ./...

fmt-check:
	@test -z "$$(gofmt -l .)" || (gofmt -l . && exit 1)

clean:
	go clean
