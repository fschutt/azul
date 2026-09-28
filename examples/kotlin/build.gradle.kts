// Hello-world application build for the Azul Kotlin binding.
// Source of truth: doc/src/codegen/v2/lang_kotlin/gradle.rs (re-run
// `azul-doc codegen all` after editing it; examples/kotlin holds a copy).
//
//   gradle build
//   java -XstartOnFirstThread -Djna.library.path=. -jar build/libs/hello-world-1.0.0.jar
//
// The binding is compiled from `Azul.kt` when that file sits next to this
// script (release-page download) or `-Pazul.codegen.dir=<dir>` names a
// generated-bindings directory; otherwise the published jar
// rs.azul:<azul.artifact>:<azul.version> is resolved from https://azul.rs/ui/maven.

plugins {
    kotlin("jvm") version "2.3.21"
    application
}

group = "com.azul.examples"
version = "1.0.0"

val azulVersion = (findProperty("azul.version") as String?) ?: "0.2.0"
val azulArtifact = (findProperty("azul.artifact") as String?) ?: "azul-kotlin"

// Directory holding a generated Azul.kt (null = use the maven artifact).
val azulCodegenDir: String? = (findProperty("azul.codegen.dir") as String?)
    ?: listOf(projectDir.path, "${projectDir}/../../target/codegen/kotlin")
        .firstOrNull { File(it, "Azul.kt").isFile }

// Directory holding libazul.dylib / libazul.so / azul.dll for `gradle run`.
val azulNativeDir: String = (findProperty("azul.native.dir") as String?)
    ?: listOf(projectDir.path, "${projectDir}/../../target/release")
        .firstOrNull { dir ->
            listOf("libazul.dylib", "libazul.so", "azul.dll").any { File(dir, it).isFile }
        }
    ?: projectDir.path

repositories {
    mavenCentral()
    maven {
        url = uri("https://azul.rs/ui/maven")
    }
}

dependencies {
    // JNA is a transitive dependency of the published jar, pinned here for
    // the from-source route.
    implementation("net.java.dev.jna:jna:5.14.0")
    if (azulCodegenDir == null) {
        implementation("rs.azul:$azulArtifact:$azulVersion")
    }
}

kotlin {
    jvmToolchain(17)
}

// Sources are assembled into one directory so the generated Azul.kt (when
// present) and HelloWorld.kt compile together without a src/main tree.
val azulSrcDir = layout.buildDirectory.dir("generated/azul-src")
val assembleAzulSources by tasks.registering(Copy::class) {
    into(azulSrcDir)
    from("${projectDir}/HelloWorld.kt")
    if (azulCodegenDir != null) {
        from("$azulCodegenDir/Azul.kt")
    }
}

sourceSets["main"].kotlin.setSrcDirs(listOf(azulSrcDir))

tasks.named("compileKotlin") {
    dependsOn(assembleAzulSources)
}

application {
    mainClass.set("com.azul.HelloWorldKt")
}

// Fat jar: `java -jar build/libs/hello-world-1.0.0.jar` needs kotlin-stdlib
// and JNA (and the binding jar on the maven route) inside the archive.
tasks.jar {
    manifest {
        attributes["Main-Class"] = "com.azul.HelloWorldKt"
    }
    duplicatesStrategy = DuplicatesStrategy.EXCLUDE
    exclude("META-INF/*.SF", "META-INF/*.DSA", "META-INF/*.RSA")
    from(configurations.runtimeClasspath.get().map { if (it.isDirectory) it else zipTree(it) })
}

tasks.named<JavaExec>("run") {
    systemProperty("jna.library.path", azulNativeDir)
    // macOS: libazul's NSApplication loop must start on the JVM main thread.
    if (System.getProperty("os.name").lowercase().contains("mac")) {
        jvmArgs("-XstartOnFirstThread")
    }
    setIgnoreExitValue(false)
}
