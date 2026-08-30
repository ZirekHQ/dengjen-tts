plugins {
    java
    `jvm-test-suite`
    alias(libs.plugins.spotless)
}

group = "dev.dengjen"
version = "0.1.0"

java {
    toolchain {
        languageVersion = JavaLanguageVersion.of(25)
    }
}

repositories {
    mavenCentral()
}

testing {
    suites {
        val test by getting(JvmTestSuite::class) {
            useJUnitJupiter(libs.versions.junit.jupiter.get())
        }
        val integrationTest by registering(JvmTestSuite::class) {
            dependencies {
                implementation(project())
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

spotless {
    java {
        googleJavaFormat()
    }
}

tasks.named("check") {
    dependsOn(testing.suites.named("integrationTest"))
    dependsOn(testing.suites.named("e2e"))
}
