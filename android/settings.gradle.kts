// ABOUTME: Gradle settings for the Sprout Android library project.
// ABOUTME: Only the sproutmobile library module is included here.
pluginManagement {
    repositories {
        google()
        mavenCentral()
        gradlePluginPortal()
    }
}

dependencyResolutionManagement {
    repositoriesMode.set(RepositoriesMode.FAIL_ON_PROJECT_REPOS)
    repositories {
        google()
        mavenCentral()
    }
}

rootProject.name = "SproutMobileAndroid"
include(":sproutmobile")
