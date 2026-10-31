// Copy target/codegen/kotlin/Azul.kt into src/main/kotlin/, then
//   gradle build && java -Djna.library.path=<dir of libazul> -jar build/libs/azul-styles.jar
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
