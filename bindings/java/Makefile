CARGO_MANIFEST := ../../Cargo.toml

.PHONY: native test clean

native:
	cargo build --release --manifest-path $(CARGO_MANIFEST) -p libdengjen

test: native
	./gradlew test integrationTest e2e

clean:
	./gradlew clean
