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
    }
}

tasks.withType<Test>().configureEach {
    jvmArgs("--enable-native-access=ALL-UNNAMED")
}

tasks.named("check") {
    dependsOn(testing.suites.named("integrationTest"))
}
