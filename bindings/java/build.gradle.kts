plugins {
    java
    `jvm-test-suite`
}

group = "dev.dengjen"
version = "0.1.0"

java {
    toolchain {
        languageVersion = JavaLanguageVersion.of(22)
    }
}

repositories {
    mavenCentral()
}

testing {
    suites {
        val test by getting(JvmTestSuite::class) {
            useJUnitJupiter("5.11.4")
        }
        val integrationTest by registering(JvmTestSuite::class) {
            dependencies {
                implementation(project())
            }
            useJUnitJupiter("5.11.4")
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
            useJUnitJupiter("5.11.4")
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

tasks.named("check") {
    dependsOn(testing.suites.named("integrationTest"))
    dependsOn(testing.suites.named("e2e"))
}
