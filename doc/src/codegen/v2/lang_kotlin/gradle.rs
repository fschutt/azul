//! Emits the Gradle Kotlin DSL manifests shipped on the release page next
//! to `Azul.kt` and `HelloWorld.kt` (`deploy.rs` BINDING_FILES): the
//! hello-world *application* build the guide describes —
//!
//! ```sh
//! gradle build
//! java -XstartOnFirstThread -Djna.library.path=. -jar build/libs/hello-world-1.0.0.jar
//! ```
//!
//! Where the binding comes from is decided at configuration time:
//!
//! * `Azul.kt` next to `build.gradle.kts` (the release-page download layout) or
//!   `-Pazul.codegen.dir=<dir>` (a repo checkout: `target/codegen/kotlin`) → compiled from source;
//! * otherwise the published jar `rs.azul:<azul.artifact>:<azul.version>` from
//!   `https://azul.rs/ui/maven`.
//!
//! The jar is FAT (kotlin-stdlib + JNA inside) so `java -jar` works as
//! documented. `examples/kotlin/build.gradle.kts` is a verbatim copy of
//! this output so the in-repo example and the shipped file cannot drift.

/// The `build.gradle.kts` body. `version` is the azul release the maven
/// route pins by default (`-Pazul.version=` overrides it at build time).
pub fn generate_build_gradle_kts() -> String {
    build_gradle_kts_with_version(DEFAULT_AZUL_VERSION)
}

/// Default `rs.azul` artifact version for the maven route. Kept in one
/// place with the Kotlin `pom.xml`'s `<azul.version>`.
pub const DEFAULT_AZUL_VERSION: &str = "0.2.0";

/// Maven artifact id of the published Kotlin binding, as the guide's
/// `implementation("rs.azul:azul:$VERSION")` names it. Overridable per build
/// with `-Pazul.artifact=`.
pub const DEFAULT_AZUL_ARTIFACT: &str = "azul";

pub fn build_gradle_kts_with_version(version: &str) -> String {
    format!(
        r#"// Hello-world application build for the Azul Kotlin binding.
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

plugins {{
    kotlin("jvm") version "2.3.21"
    application
}}

group = "com.azul.examples"
version = "1.0.0"

val azulVersion = (findProperty("azul.version") as String?) ?: "{version}"
val azulArtifact = (findProperty("azul.artifact") as String?) ?: "{artifact}"

// Directory holding a generated Azul.kt (null = use the maven artifact).
val azulCodegenDir: String? = (findProperty("azul.codegen.dir") as String?)
    ?: listOf(projectDir.path, "${{projectDir}}/../../target/codegen/kotlin")
        .firstOrNull {{ File(it, "Azul.kt").isFile }}

// Directory holding libazul.dylib / libazul.so / azul.dll for `gradle run`.
val azulNativeDir: String = (findProperty("azul.native.dir") as String?)
    ?: listOf(projectDir.path, "${{projectDir}}/../../target/release")
        .firstOrNull {{ dir ->
            listOf("libazul.dylib", "libazul.so", "azul.dll").any {{ File(dir, it).isFile }}
        }}
    ?: projectDir.path

repositories {{
    mavenCentral()
    maven {{
        url = uri("https://azul.rs/ui/maven")
    }}
}}

dependencies {{
    // JNA is a transitive dependency of the published jar, pinned here for
    // the from-source route.
    implementation("net.java.dev.jna:jna:5.14.0")
    if (azulCodegenDir == null) {{
        implementation("rs.azul:$azulArtifact:$azulVersion")
    }}
}}

kotlin {{
    jvmToolchain(17)
}}

// Sources are assembled into one directory so the generated Azul.kt (when
// present) and HelloWorld.kt compile together without a src/main tree.
val azulSrcDir = layout.buildDirectory.dir("generated/azul-src")
val assembleAzulSources by tasks.registering(Copy::class) {{
    into(azulSrcDir)
    from("${{projectDir}}/HelloWorld.kt")
    if (azulCodegenDir != null) {{
        from("$azulCodegenDir/Azul.kt")
    }}
}}

sourceSets["main"].kotlin.setSrcDirs(listOf(azulSrcDir))

tasks.named("compileKotlin") {{
    dependsOn(assembleAzulSources)
}}

application {{
    mainClass.set("com.azul.HelloWorldKt")
}}

// Fat jar: `java -jar build/libs/hello-world-1.0.0.jar` needs kotlin-stdlib
// and JNA (and the binding jar on the maven route) inside the archive.
tasks.jar {{
    manifest {{
        attributes["Main-Class"] = "com.azul.HelloWorldKt"
    }}
    duplicatesStrategy = DuplicatesStrategy.EXCLUDE
    exclude("META-INF/*.SF", "META-INF/*.DSA", "META-INF/*.RSA")
    from(configurations.runtimeClasspath.get().map {{ if (it.isDirectory) it else zipTree(it) }})
}}

tasks.named<JavaExec>("run") {{
    systemProperty("jna.library.path", azulNativeDir)
    // macOS: libazul's NSApplication loop must start on the JVM main thread.
    if (System.getProperty("os.name").lowercase().contains("mac")) {{
        jvmArgs("-XstartOnFirstThread")
    }}
    setIgnoreExitValue(false)
}}
"#,
        version = version,
        artifact = DEFAULT_AZUL_ARTIFACT,
    )
}

/// Companion `settings.gradle.kts`: names the project so the jar is
/// `build/libs/hello-world-1.0.0.jar`, as the guide's run command expects.
pub fn generate_settings_gradle_kts() -> String {
    "rootProject.name = \"hello-world\"\n".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The checked-in example must be byte-identical to the shipped file.
    #[test]
    fn example_build_gradle_matches_generated() {
        let example = include_str!("../../../../../examples/kotlin/build.gradle.kts");
        assert_eq!(example, generate_build_gradle_kts());
        let settings = include_str!("../../../../../examples/kotlin/settings.gradle.kts");
        assert_eq!(settings, generate_settings_gradle_kts());
    }
}
