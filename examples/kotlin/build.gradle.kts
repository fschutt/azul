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

val azulCodegenDir = (findProperty("azul.codegen.dir") as String?)
    ?: "${projectDir}/../../target/codegen/kotlin"
val azulNativeDir = (findProperty("azul.native.dir") as String?)
    ?: "${projectDir}/../../target/release"

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

tasks.named<JavaExec>("run") {
    systemProperty("jna.library.path", azulNativeDir)
    setIgnoreExitValue(false)
}
