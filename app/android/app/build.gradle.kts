import java.util.Properties

plugins {
    id("com.android.application")
    // The Flutter Gradle Plugin must be applied after the Android and Kotlin Gradle plugins.
    id("dev.flutter.flutter-gradle-plugin")
}

android {
    // Python and QuickJS are executables shipped as native libraries; Android must unpack them.
    packaging { jniLibs { useLegacyPackaging = true } }
    namespace = "dev.harp.harp"
    compileSdk = flutter.compileSdkVersion
    ndkVersion = flutter.ndkVersion

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }

    defaultConfig {
        // TODO: Specify your own unique Application ID (https://developer.android.com/studio/build/application-id.html).
        applicationId = "dev.harp.harp"
        // You can update the following values to match your application needs.
        // For more information, see: https://flutter.dev/to/review-gradle-config.
        minSdk = 28 // the bundled Python and QuickJS need getrandom (Android 9)
        targetSdk = flutter.targetSdkVersion
        // Uses the version code from pubspec.yaml. When using split APKs, 1000 * ABI_VERSION
        // is added automatically by Flutter. (https://developer.android.com/studio/build/configure-apk-splits#configure-APK-versions)
        // You can force using the value of versionCode by specifying the `-P force-version-code-ignoring-abi=true`
        // flag during build.
        versionCode = flutter.versionCode
        versionName = flutter.versionName
    }

    // Release signing: the key lives outside the repo. Locally its details are in
    // ~/.local/share/harp-signing/key.properties; on GitHub Actions in secrets
    // (HARP_KEYSTORE_FILE, HARP_KEYSTORE_PASSWORD, HARP_KEY_ALIAS, HARP_KEY_PASSWORD).
    // Without either, release builds fall back to the debug key (development only).
    val keyProps = Properties().apply {
        val f = file(System.getProperty("user.home") + "/.local/share/harp-signing/key.properties")
        if (f.exists()) f.inputStream().use { load(it) }
        System.getenv("HARP_KEYSTORE_FILE")?.let { put("storeFile", it) }
        System.getenv("HARP_KEYSTORE_PASSWORD")?.let { put("storePassword", it) }
        System.getenv("HARP_KEY_ALIAS")?.let { put("keyAlias", it) }
        System.getenv("HARP_KEY_PASSWORD")?.let { put("keyPassword", it) }
    }
    val haveReleaseKey = keyProps.getProperty("storeFile")?.let { file(it).exists() } == true

    signingConfigs {
        if (haveReleaseKey) {
            create("release") {
                storeFile = file(keyProps.getProperty("storeFile"))
                storePassword = keyProps.getProperty("storePassword")
                keyAlias = keyProps.getProperty("keyAlias")
                keyPassword = keyProps.getProperty("keyPassword")
            }
        }
    }

    buildTypes {
        release {
            signingConfig = if (haveReleaseKey) signingConfigs.getByName("release") else signingConfigs.getByName("debug")
        }
    }
}

kotlin {
    compilerOptions {
        jvmTarget = org.jetbrains.kotlin.gradle.dsl.JvmTarget.JVM_17
    }
}

flutter {
    source = "../.."
}
