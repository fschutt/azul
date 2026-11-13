// Copy target/codegen/kotlin/Azul.kt into src/main/kotlin/, then
//   gradle build && java -Djna.library.path=<dir of libazul> -jar build/libs/azul-app.jar
plugins {
    kotlin("jvm") version "2.3.21"
    application
}

repositories {
    mavenCentral()
}

dependencies {
    implementation("net.java.dev.jna:jna:5.14.0")
}

application {
    mainClass.set("com.azul.MainKt")
}

// A runnable jar: kotlin-stdlib and JNA inside, the main class in the
// manifest. macOS: java -XstartOnFirstThread -Djna.library.path=.. -jar ..
tasks.jar {
    manifest {
        attributes["Main-Class"] = "com.azul.MainKt"
    }
    duplicatesStrategy = DuplicatesStrategy.EXCLUDE
    exclude("META-INF/*.SF", "META-INF/*.DSA", "META-INF/*.RSA")
    from(configurations.runtimeClasspath.get().map { if (it.isDirectory) it else zipTree(it) })
}
