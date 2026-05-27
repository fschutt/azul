// Hello-world Gradle (Kotlin DSL) project for the Azul Kotlin
// bindings.
//
// The generated Kotlin bindings are a single file emitted by
//     cargo run -r -p azul-doc codegen all
// into  target/codegen/kotlin/Azul.kt  (package com.azul). Rather than
// publishing a `com.azul:azul` artifact to mavenLocal, this build adds
// that generated file directly as a source so the example always
// compiles against the freshly-generated API.
//
// Build + run:
//     cargo run -r -p azul-doc codegen all
//     cargo build  (release, -p azul-dll, feature build-dll)
//     cd examples/kotlin && gradle run
//
// The native libazul (.dylib/.so/.dll) is located by JNA via the
// jna.library.path system property wired onto the `run` task below
// (defaults to ../../target/release). The AZ_E2E / AZ_BACKEND env vars
// are inherited by the forked run JVM and drive the headless counter
// test inside libazul; its process exit code becomes the build result.
//
// Kotlin Gradle plugin 2.3.x is the first line that supports Gradle 9.x
// (Gradle >= 9.0 dropped support for KGP < 2.0; KGP 2.3.20+ is required
// for Gradle 9.0-9.3). Override the native/codegen dirs with
// -Pazul.native.dir=... / -Pazul.codegen.dir=... if your checkout differs.

plugins {
    kotlin("jvm") version "2.3.21"
    application
}

group = "com.azul.examples"
version = "1.0.0"

repositories {
    mavenCentral()
}

dependencies {
    implementation("net.java.dev.jna:jna:5.14.0")
}

// No `jvmToolchain(N)` pin: that forces Gradle to locate (or download)
// a specific JDK, which fails on machines/CI that only have a newer JDK
// installed. Compile and run with whatever JDK is running Gradle.

// Directory holding the generated com.azul.* bindings (Azul.kt) and the
// prebuilt native libazul. Relative to this project (examples/kotlin),
// overridable via -P project properties.
val azulCodegenDir = (findProperty("azul.codegen.dir") as String?)
    ?: "${projectDir}/../../target/codegen/kotlin"
val azulNativeDir = (findProperty("azul.native.dir") as String?)
    ?: "${projectDir}/../../target/release"

// Assemble the two Kotlin sources we actually compile — the example's
// HelloWorld.kt and the freshly-generated Azul.kt — into a single clean
// generated-sources directory, then point the main source set at ONLY
// that directory. This deliberately avoids scanning the project root,
// so neither the build scripts (build.gradle.kts / settings.gradle.kts)
// nor a stale local copy of the bindings (examples/kotlin/Azul.kt, which
// is git-ignored and may linger from a prior `cp`) can leak in and cause
// "Redeclaration" / duplicate-source errors.
val azulSrcDir = layout.buildDirectory.dir("generated/azul-src")
val assembleAzulSources by tasks.registering(Copy::class) {
    into(azulSrcDir)
    from("$azulCodegenDir/Azul.kt")
    from("${projectDir}/HelloWorld.kt")
}

sourceSets["main"].kotlin.setSrcDirs(listOf(azulSrcDir))

tasks.named("compileKotlin") {
    dependsOn(assembleAzulSources)
}

application {
    mainClass.set("com.azul.HelloWorldKt")
}

// JNA resolves libazul via jna.library.path; on macOS DYLD_LIBRARY_PATH
// is stripped from forked JVMs by SIP, so set the property explicitly.
tasks.named<JavaExec>("run") {
    systemProperty("jna.library.path", azulNativeDir)
    // The libazul E2E runner calls process::exit(); let that code be the
    // build result rather than masking it as a Gradle failure.
    setIgnoreExitValue(false)
}
