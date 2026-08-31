CARGO_MANIFEST := ../../Cargo.toml

.PHONY: native test coverage fmt-check clean

native:
	cargo build --release --manifest-path $(CARGO_MANIFEST) -p libdengjen

test: native
	./gradlew test integrationTest e2e

# jacocoTestReport depends on test/integrationTest/e2e (see build.gradle.kts) and merges all
# three into the XML sonar.coverage.jacoco.xmlReportPaths (see ../../sonar-project.properties)
# reads directly.
coverage: native
	./gradlew jacocoTestReport

fmt-check:
	./gradlew spotlessCheck

clean:
	./gradlew clean
