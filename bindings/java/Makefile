CARGO_MANIFEST := ../../Cargo.toml

.PHONY: native test fmt-check clean

native:
	cargo build --release --manifest-path $(CARGO_MANIFEST) -p libdengjen

test: native
	./gradlew test integrationTest e2e

fmt-check:
	./gradlew spotlessCheck

clean:
	./gradlew clean
