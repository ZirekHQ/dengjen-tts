import org.gradle.internal.os.OperatingSystem

plugins {
    java
    jacoco
    `jvm-test-suite`
    alias(libs.plugins.spotless)
}

group = "io.github.zirekhq.dengjen"
version = "0.1.0"

fun currentNativeClassifier(): String {
    val os = OperatingSystem.current()
    val arch = System.getProperty("os.arch").lowercase()
    val isArm64 = arch == "aarch64" || arch == "arm64"
    val isX64 = arch == "x86_64" || arch == "amd64" || arch == "x64"
    return when {
        os.isWindows && isX64 -> "windows-x64"
        os.isMacOsX && isArm64 -> "macos-aarch64"
        os.isLinux && isArm64 -> "linux-aarch64"
        os.isLinux && isX64 -> "linux-x86_64"
        else -> throw GradleException("nativeTestJar: unsupported platform (os=$os, arch=$arch)")
    }
}

// Packages the current platform's already-built libdengjen (see `make native`) as a classifier
// jar, using the exact natives/<classifier>/<file> layout NativeLibraryLoader looks for and the
// real per-platform CI classifier jars (Task 6) will produce. Exists so the loader can be proven
// against a real native library locally, without any publishing infrastructure.
val nativeTestJar by tasks.registering(Jar::class) {
    archiveBaseName.set("dengjen-java-bindings-native-test")
    archiveClassifier.set(currentNativeClassifier())
    val nativeLibraryFile = file("../../target/release/${System.mapLibraryName("libdengjen")}")
    from(nativeLibraryFile) { into("natives/${currentNativeClassifier()}") }
    doFirst {
        check(nativeLibraryFile.exists()) {
            "libdengjen not built at $nativeLibraryFile -- run `make native` first"
        }
    }
}

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
        val nativeLoaderTest by registering(JvmTestSuite::class) {
            dependencies {
                implementation(project())
                implementation(libs.assertj.core)
            }
            useJUnitJupiter(libs.versions.junit.jupiter.get())
            targets {
                all {
                    testTask.configure {
                        dependsOn(nativeTestJar)
                        // Deliberately NOT given dengjen.native.library.path (unlike test/integrationTest/e2e in
                        // Task 3) -- this suite exists specifically to force NativeLibraryLoader through its
                        // classpath-resource path, not the override.
                        classpath += files(nativeTestJar.map { it.archiveFile })
                        jvmArgs("--enable-native-access=ALL-UNNAMED")
                        shouldRunAfter(tasks.test)
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
