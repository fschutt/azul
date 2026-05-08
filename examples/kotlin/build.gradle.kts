// Hello-world Gradle (Kotlin DSL) project for the Azul Kotlin
// bindings. Place the prebuilt native library under
// src/main/resources/{linux-x86-64,win32-x86-64,darwin}/ before running.
//
//     ./gradlew run

plugins {
    kotlin("jvm") version "1.9.22"
    application
}

group = "com.azul.examples"
version = "1.0.0"

repositories {
    mavenCentral()
    mavenLocal() // For locally-installed `azul:1.0.0` JAR.
}

dependencies {
    implementation("net.java.dev.jna:jna:5.14.0")
    implementation("com.azul:azul:1.0.0")
}

kotlin {
    jvmToolchain(11)
}

application {
    mainClass.set("HelloWorldKt")
}

sourceSets["main"].kotlin.srcDir(".")
sourceSets["main"].kotlin.include("HelloWorld.kt")
