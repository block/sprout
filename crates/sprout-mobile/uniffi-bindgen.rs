// ABOUTME: Binary entry point for generating Swift and Kotlin bindings.
// ABOUTME: Invoke via: cargo run -p sprout-mobile --bin uniffi-bindgen -- generate ...

fn main() {
    uniffi::uniffi_bindgen_main();
}
