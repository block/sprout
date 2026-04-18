fn main() {
    println!("cargo:rerun-if-env-changed=SPROUT_RELAY_URL");
    println!("cargo:rerun-if-env-changed=SPROUT_RELAY_HTTP");

    if let Ok(relay_url) = std::env::var("SPROUT_RELAY_URL") {
        println!("cargo:rustc-env=SPROUT_DESKTOP_BUILD_RELAY_URL={relay_url}");
    }

    if let Ok(relay_http) = std::env::var("SPROUT_RELAY_HTTP") {
        println!("cargo:rustc-env=SPROUT_DESKTOP_BUILD_RELAY_HTTP={relay_http}");
    }

    tauri_build::build()
}
