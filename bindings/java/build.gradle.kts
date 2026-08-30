plugins {
    java
    `jvm-test-suite`
    id("com.diffplug.spotless") version "8.10.1"
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

spotless {
    java {
        target("src/*/java/**/*.java")
        // Pinned to 1.28.0: later releases (confirmed as of 1.36.1) ship
        // class files requiring JDK 21+ to run, but Spotless runs the
        // formatter in the Gradle daemon's own JVM, which may be JDK 17 --
        // 1.28.0 is confirmed to run there.
        googleJavaFormat("1.28.0")
    }
}

tasks.named("check") {
    dependsOn(testing.suites.named("integrationTest"))
    dependsOn(testing.suites.named("e2e"))
}
