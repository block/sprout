# ABOUTME: Consumer ProGuard rules — applied to apps that depend on this library.
# ABOUTME: Keeps JNA and UniFFI-generated classes so reflection-based lookups survive minification.
-keep class com.sun.jna.** { *; }
-keep class video.divine.sprout.mobile.uniffi.** { *; }
