import org.gradle.internal.os.OperatingSystem

// Spotless (8.10.1) pulls jgit 7.7.1 onto this project's shared plugin classpath, which wins
// Gradle's default "highest version" conflict resolution over the jgit 5.13.5 that
// jreleaser-git-java-sdk actually declares -- but jgit 7.x removed org.eclipse.jgit.lib
// .GpgObjectSigner, which JReleaser's ModelConfigurer still references, so jreleaserConfig fails
// with NoClassDefFoundError unless jgit is pinned back to a version that still has it.
buildscript {
    configurations.classpath {
        resolutionStrategy {
            force("org.eclipse.jgit:org.eclipse.jgit:5.13.5.202508271544-r")
        }
    }
}

plugins {
    java
    jacoco
    `jvm-test-suite`
    `maven-publish`
    alias(libs.plugins.git.version)
    alias(libs.plugins.jreleaser)
    alias(libs.plugins.spotless)
}

group = "io.github.zirekhq.dengjen"
version = "0.0.0-SNAPSHOT"

val snapshotVersion =
    "\${describe.tag.version.major}.\${describe.tag.version.minor}.\${describe.tag.version.patch.next}-SNAPSHOT"

gitVersioning.apply {
    refs {
        branch("main") { version = snapshotVersion }
        tag("java-v(?<version>.*)") { version = "\${ref.version}" }
    }
    // Without this, the snapshot fallback's `git describe` matches ANY tag in the repo -- including
    // the unrelated crates.io release tags -- instead of just the java-v* tags this module owns.
    describeTagPattern = "java-v.*"
    rev { version = snapshotVersion }
}

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
    withSourcesJar()
    withJavadocJar()
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

val stagingDir: Provider<Directory> = layout.buildDirectory.dir("staging-deploy")

publishing {
    publications {
        create<MavenPublication>("maven") {
            from(components["java"])
            pom {
                name.set("dengjen-java-bindings")
                description.set("Java bindings for libdengjen, the C API for dengjen-tts.")
                url.set("https://github.com/ZirekHQ/dengjen-tts")
                licenses {
                    license {
                        name.set("GPL-3.0-or-later")
                        url.set("https://www.gnu.org/licenses/gpl-3.0.txt")
                        distribution.set("repo")
                    }
                }
                scm {
                    connection.set("scm:git:https://github.com/ZirekHQ/dengjen-tts.git")
                    developerConnection.set("scm:git:git@github.com:ZirekHQ/dengjen-tts.git")
                    url.set("https://github.com/ZirekHQ/dengjen-tts.git")
                }
                developers {
                    developer {
                        id.set("austek")
                        name.set("Ali Ustek")
                    }
                }
            }
        }
    }
    repositories {
        maven { url = uri(stagingDir.get()) }
    }
}

tasks.named("check") {
    dependsOn(testing.suites.named("integrationTest"))
    dependsOn(testing.suites.named("e2e"))
}

val nativeClassifiers = listOf("linux-x86_64", "linux-aarch64", "windows-x64", "macos-aarch64")

val nativeArtifactsDir: Directory =
    layout.projectDirectory.dir((findProperty("nativeArtifactsDir") as String?) ?: "native-artifacts")

val nativeClassifierJars =
    nativeClassifiers.associateWith { classifier ->
        tasks.register<Jar>("nativeJar-$classifier") {
            archiveClassifier.set(classifier)
            val sourceDir = nativeArtifactsDir.dir(classifier)
            from(sourceDir) { into("natives/$classifier") }
            onlyIf { sourceDir.asFile.exists() }
            // Maven Central is immutable -- an empty or wrong-content classifier jar would burn
            // that version forever with a native library nobody can load, so fail loudly instead
            // of silently publishing whatever (or nothing) happens to be in the directory.
            doFirst {
                val files = sourceDir.asFile.listFiles().orEmpty()
                check(files.size == 1) {
                    "expected exactly one native library file in $sourceDir, found ${files.toList()}"
                }
            }
        }
    }

publishing {
    publications {
        named<MavenPublication>("maven") {
            nativeClassifierJars.values.forEach { jarTask -> artifact(jarTask) }
        }
    }
}

configure<org.jreleaser.gradle.plugin.JReleaserExtension> {
    // bindings/java is a subdirectory of the dengjen-tts monorepo, not the git root -- without
    // this, JReleaser's git detection only looks at basedir and fails with
    // "repository not found: .../bindings/java" instead of walking up to find the repo's .git.
    gitRootSearch = true
    release {
        github {
            // The java-v* tag is pushed manually before this workflow ever runs (see Task 9) --
            // JReleaser must not try to create it again.
            skipTag = true
            // JReleaser defaults tagName to "v{{projectVersion}}", but this workflow's tags are
            // java-v* -- without this, a real tag push creates a stray v<version> GitHub
            // tag/release instead of matching the actual tag, and changelog commit-range
            // resolution breaks too.
            tagName = "java-v{{projectVersion}}"
            changelog {
                formatted = org.jreleaser.model.Active.ALWAYS
                preset = "conventional-commits"
                links = true
            }
        }
    }
    signing {
        pgp {
            active = org.jreleaser.model.Active.ALWAYS
            armored = true
        }
    }
    deploy {
        maven {
            mavenCentral {
                register("sonatype") {
                    active = org.jreleaser.model.Active.ALWAYS
                    url = "https://central.sonatype.com/api/v1/publisher"
                    stagingRepository(stagingDir.get().toString())
                }
            }
        }
    }
}
