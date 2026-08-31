plugins {
    java
    jacoco
    `jvm-test-suite`
    alias(libs.plugins.spotless)
}

group = "io.github.zirekhq.dengjen"
version = "0.1.0"

java {
    toolchain {
        languageVersion = JavaLanguageVersion.of(25)
    }
}

repositories {
    mavenCentral()
}

dependencyLocking {
    lockAllConfigurations()
}

testing {
    suites {
        val test by getting(JvmTestSuite::class) {
            dependencies {
                implementation(libs.assertj.core)
            }
            useJUnitJupiter(libs.versions.junit.jupiter.get())
        }
        val integrationTest by registering(JvmTestSuite::class) {
            dependencies {
                implementation(project())
                implementation(libs.assertj.core)
                implementation(libs.awaitility)
            }
            useJUnitJupiter(libs.versions.junit.jupiter.get())
            targets {
                all {
                    testTask.configure {
                        shouldRunAfter(test)
                    }
                }
            }
        }
        // Exercises the bindings against a real trained voice; soft-skips
        // (JUnit Assumptions.assumeTrue) when DENGJEN_KOKORO_TEST_VOICE_CONFIG
        // is unset, same convention as the Rust *_e2e_real_voice.rs tests.
        val e2e by registering(JvmTestSuite::class) {
            dependencies {
                implementation(project())
                implementation(libs.assertj.core)
            }
            useJUnitJupiter(libs.versions.junit.jupiter.get())
            targets {
                all {
                    testTask.configure {
                        shouldRunAfter(integrationTest)
                    }
                }
            }
        }
    }
}

tasks.withType<Test>().configureEach {
    jvmArgs("--enable-native-access=ALL-UNNAMED")
}

// NativeLibraryLoader looks for a classifier jar on the classpath by default, which none of these
// three suites carry -- point them at the native library `make native` builds instead, exactly
// reproducing DengjenLib's old hardcoded ../../target/release resolution, just routed through the
// same override property a published consumer would use.
val devNativeLibraryPath = file("../../target/release/${System.mapLibraryName("libdengjen")}").absolutePath

listOf("test", "integrationTest", "e2e").forEach { suiteName ->
    tasks.named<Test>(suiteName) {
        systemProperty("dengjen.native.library.path", devNativeLibraryPath)
    }
}

// The jacoco plugin instruments every Test task (unit test, integrationTest, e2e) into its own
// *.exec file under build/jacoco/; jacocoTestReport only reads test.exec by default, so it's
// pointed at all three -- and depends on them running first -- to get a report covering the
// whole suite rather than just the unit tests.
tasks.jacocoTestReport {
    dependsOn(tasks.test, testing.suites.named("integrationTest"), testing.suites.named("e2e"))
    executionData(fileTree(layout.buildDirectory.dir("jacoco")).include("*.exec"))
    reports {
        xml.required.set(true)
        html.required.set(false)
    }
}

spotless {
    java {
        googleJavaFormat()
    }
}

tasks.named("check") {
    dependsOn(testing.suites.named("integrationTest"))
    dependsOn(testing.suites.named("e2e"))
}
