# Sprout — development task runner

set dotenv-load := true

desktop_dir := "desktop"
desktop_tauri_manifest := "desktop/src-tauri/Cargo.toml"

# List all available tasks
default:
    @just --list

# ─── Dev Environment ─────────────────────────────────────────────────────────

# Start Docker services, run migrations, install desktop deps
setup:
    ./scripts/dev-setup.sh

# ⚠️  Wipe ALL data and recreate a clean environment
[confirm("This will DELETE all local data. Continue? (y/N)")]
reset:
    ./scripts/dev-reset.sh --yes

# Stop all dev services (keep data)
down:
    docker compose down

# Show dev service status
ps:
    docker compose ps

# Tail all service logs
logs *ARGS:
    docker compose logs -f {{ARGS}}

# ─── Build & Check ───────────────────────────────────────────────────────────

# Build the Rust workspace
build:
    cargo build --workspace

# Build the Rust workspace in release mode
build-release:
    cargo build --workspace --release

# Run repo lint and formatting checks
check: fmt-check clippy desktop-check desktop-tauri-fmt-check

# Format all Rust code
fmt:
    cargo fmt --all

# Check formatting without modifying files
fmt-check:
    cargo fmt --all -- --check

# Run clippy with warnings as errors
clippy:
    cargo clippy --workspace --all-targets -- -D warnings

# Install desktop JS dependencies
desktop-install:
    cd {{desktop_dir}} && pnpm install

# Install desktop JS dependencies reproducibly for CI
desktop-install-ci:
    cd {{desktop_dir}} && pnpm install --frozen-lockfile

# Run desktop lint and format checks
desktop-check:
    cd {{desktop_dir}} && pnpm check

# Run desktop TypeScript checks
desktop-typecheck:
    cd {{desktop_dir}} && pnpm typecheck

# Build desktop frontend assets
desktop-build:
    cd {{desktop_dir}} && pnpm build

# Format desktop Tauri Rust code
desktop-tauri-fmt:
    cargo fmt --manifest-path {{desktop_tauri_manifest}} --all

# Check desktop Tauri Rust formatting
desktop-tauri-fmt-check:
    cargo fmt --manifest-path {{desktop_tauri_manifest}} --all -- --check

# Check the desktop Tauri Rust crate compiles
desktop-tauri-check:
    cargo check --manifest-path {{desktop_tauri_manifest}}

# Run desktop checks suitable for CI / pre-push
desktop-ci: desktop-check desktop-tauri-fmt-check desktop-build desktop-tauri-check

# Seed deterministic channel data for desktop Playwright tests
desktop-e2e-seed:
    ./scripts/setup-desktop-test-data.sh

# Run desktop browser smoke tests
desktop-e2e-smoke:
    cd {{desktop_dir}} && pnpm test:e2e:smoke

# Run desktop relay-backed e2e tests
desktop-e2e-integration:
    cd {{desktop_dir}} && pnpm test:e2e:integration

# Run all checks suitable for CI / pre-push (no infra needed)
ci: check test-unit desktop-build desktop-tauri-check

# ─── Test ─────────────────────────────────────────────────────────────────────

# Run all tests (unit + integration)
test:
    ./scripts/run-tests.sh all

# Run unit tests only (no infra needed)
test-unit:
    ./scripts/run-tests.sh unit

# Run integration tests only (starts services if needed)
test-integration:
    ./scripts/run-tests.sh integration

# ─── Run ──────────────────────────────────────────────────────────────────────

# Start the relay server
relay:
    cargo run -p sprout-relay

# Start the relay server in release mode
relay-release:
    cargo run -p sprout-relay --release

# Start sprout-proxy (dev mode)
proxy:
    cargo run -p sprout-proxy

# Start sprout-proxy (release mode)
proxy-release:
    cargo run -p sprout-proxy --release

# Run the desktop Tauri app in dev mode (uses dev identifier for side-by-side with production)
dev *ARGS:
    cd {{desktop_dir}} && pnpm tauri dev --config src-tauri/tauri.dev.conf.json {{ARGS}}

# Run the desktop frontend dev server
desktop-dev:
    cd {{desktop_dir}} && pnpm dev

# Run the desktop Tauri app (uses dev identifier for side-by-side with production)
desktop-app *ARGS:
    cd {{desktop_dir}} && pnpm tauri dev --config src-tauri/tauri.dev.conf.json {{ARGS}}

# ─── Desktop Release ──────────────────────────────────────────────────────────

# Tag and push to trigger the desktop release workflow (CI handles version bumping)
desktop-release version:
    #!/usr/bin/env bash
    set -euo pipefail

    git tag "desktop/v{{version}}"
    git push origin "desktop/v{{version}}"

    echo "Pushed tag desktop/v{{version}} — CI will set the version, build, and publish the release."

# Set the desktop app version (patches package.json, tauri.conf.json, Cargo.toml)
desktop-set-version version:
    cd {{desktop_dir}} && node scripts/set-version-from-tag.mjs "{{version}}"
    cd {{desktop_dir}}/src-tauri && cargo generate-lockfile

# Build a local desktop release (for testing). Optionally set a version first.
desktop-release-build version="" target="aarch64-apple-darwin" *args:
    #!/usr/bin/env bash
    set -euo pipefail
    if [ -n "{{version}}" ]; then
        just desktop-set-version "{{version}}"
    fi
    cd {{desktop_dir}} && pnpm exec tauri build --target {{target}} --config src-tauri/tauri.release.conf.json {{args}}

# ─── Database ─────────────────────────────────────────────────────────────────

# Apply schema migrations via pgschema
migrate:
    ./bin/pgschema apply --file schema/schema.sql --auto-approve

# ─── Utilities ────────────────────────────────────────────────────────────────

# Remove build artifacts
clean:
    cargo clean
    cargo clean --manifest-path desktop/src-tauri/Cargo.toml

# Check the Rust workspace compiles without producing binaries
check-compile:
    cargo check --workspace --all-targets

# ─── Mobile (iOS / Android) ───────────────────────────────────────────────────

mobile_crate := "sprout-mobile"
mobile_lib_name := "libsprout_mobile"
ios_targets := "aarch64-apple-ios aarch64-apple-ios-sim x86_64-apple-ios"
android_targets := "aarch64-linux-android armv7-linux-androideabi x86_64-linux-android i686-linux-android"
ios_out := "ios/Frameworks/SproutCore.xcframework"
ios_swift_out := "ios/Sources/SproutMobile"
android_jni_root := "android/sproutmobile/src/main/jniLibs"
android_kotlin_out := "android/sproutmobile/src/main/java"

# Install all iOS and Android rustup targets needed for mobile builds
mobile-targets:
    rustup target add aarch64-apple-ios aarch64-apple-ios-sim x86_64-apple-ios
    rustup target add aarch64-linux-android armv7-linux-androideabi x86_64-linux-android i686-linux-android

# Verify and install prerequisites for mobile builds (cargo-ndk, SDK, NDK, JDK, Gradle wrapper)
mobile-setup:
    #!/usr/bin/env bash
    set -euo pipefail
    echo "==> Checking mobile build prerequisites…"

    missing=0

    # Rust targets — easy, install unconditionally.
    just mobile-targets

    # cargo-ndk — required for the Android build.
    if ! command -v cargo-ndk >/dev/null 2>&1; then
        echo "• cargo-ndk not found — installing…"
        cargo install cargo-ndk
    else
        echo "• cargo-ndk: $(cargo ndk --version 2>/dev/null | tail -1)"
    fi

    # Android SDK / NDK — look in common locations.
    if [ -z "${ANDROID_HOME:-}" ]; then
        for p in \
            "/opt/homebrew/share/android-commandlinetools" \
            "$HOME/Library/Android/sdk" \
            "/usr/local/share/android-commandlinetools"; do
            if [ -d "$p" ]; then
                echo "• Found Android SDK at $p"
                echo "  → export ANDROID_HOME=\"$p\""
                break
            fi
        done
    else
        echo "• ANDROID_HOME=$ANDROID_HOME"
    fi

    if [ -z "${ANDROID_NDK_HOME:-}" ]; then
        sdk_root="${ANDROID_HOME:-/opt/homebrew/share/android-commandlinetools}"
        ndk_dir="$sdk_root/ndk"
        if [ -d "$ndk_dir" ]; then
            latest_ndk=$(ls -1 "$ndk_dir" 2>/dev/null | sort -V | tail -1 || true)
            if [ -n "$latest_ndk" ]; then
                echo "• Found NDK $latest_ndk at $ndk_dir/$latest_ndk"
                echo "  → export ANDROID_NDK_HOME=\"$ndk_dir/$latest_ndk\""
            else
                echo "• NDK directory exists but is empty — run 'sdkmanager --install \"ndk;25.1.8937393\"'"
                missing=1
            fi
        else
            echo "• NDK not found — run 'sdkmanager --install \"ndk;25.1.8937393\"'"
            missing=1
        fi
    else
        echo "• ANDROID_NDK_HOME=$ANDROID_NDK_HOME"
    fi

    # JDK — AGP 8.5.2 needs JDK 17. Look via java_home first (Oracle/Temurin pkgs),
    # then check brew's openjdk@17, then fall back to whatever 'java' resolves to.
    if [ -x /usr/libexec/java_home ] && /usr/libexec/java_home -v 17 >/dev/null 2>&1; then
        jdk17_path=$(/usr/libexec/java_home -v 17)
        echo "• JDK 17 available at $jdk17_path"
        echo "  → export JAVA_HOME=\"$jdk17_path\""
    elif [ -x /opt/homebrew/opt/openjdk@17/bin/java ]; then
        echo "• JDK 17 available at /opt/homebrew/opt/openjdk@17"
        echo "  → export JAVA_HOME=\"/opt/homebrew/opt/openjdk@17/libexec/openjdk.jdk/Contents/Home\""
    elif command -v java >/dev/null 2>&1; then
        jver=$(java -version 2>&1 | head -1)
        echo "• java found ($jver) — install JDK 17 with 'brew install openjdk@17' if Gradle complains"
    else
        echo "• java not found — install with 'brew install openjdk@17'"
        missing=1
    fi

    # Gradle wrapper — check whether android/gradlew exists.
    if [ -x "android/gradlew" ]; then
        echo "• Gradle wrapper: android/gradlew"
    elif command -v gradle >/dev/null 2>&1; then
        echo "• Gradle wrapper missing — generating from installed gradle…"
        (cd android && gradle wrapper --gradle-version 8.10.2 --distribution-type bin)
    else
        echo "• Gradle wrapper missing and gradle not installed — run 'brew install gradle' then 'just mobile-setup' again"
        missing=1
    fi

    if [ "$missing" -ne 0 ]; then
        echo ""
        echo "⚠  Some prerequisites need manual action — see messages above."
        exit 1
    fi
    echo ""
    echo "✓ Mobile build prerequisites OK."

# Build the Rust core for all iOS targets and assemble an XCFramework
mobile-ios:
    #!/usr/bin/env bash
    set -euo pipefail
    echo "==> Building sprout-mobile for iOS targets…"
    for t in {{ios_targets}}; do
        echo "  • $t"
        cargo build -p {{mobile_crate}} --release --target "$t"
    done

    # Combine the two simulator slices (arm64 + x86_64) into a single fat lib.
    mkdir -p target/ios-sim-universal/release
    lipo -create \
        "target/aarch64-apple-ios-sim/release/{{mobile_lib_name}}.a" \
        "target/x86_64-apple-ios/release/{{mobile_lib_name}}.a" \
        -output "target/ios-sim-universal/release/{{mobile_lib_name}}.a"

    # Assemble the XCFramework (one slice per platform variant).
    rm -rf "{{ios_out}}"
    mkdir -p "$(dirname {{ios_out}})"
    xcodebuild -create-xcframework \
        -library "target/aarch64-apple-ios/release/{{mobile_lib_name}}.a" \
        -library "target/ios-sim-universal/release/{{mobile_lib_name}}.a" \
        -output "{{ios_out}}"

    just mobile-swift-bindings
    echo "==> iOS XCFramework built at {{ios_out}}"

# Generate Swift UniFFI bindings from the built Rust library
mobile-swift-bindings:
    #!/usr/bin/env bash
    set -euo pipefail
    echo "==> Generating Swift bindings…"
    mkdir -p "{{ios_swift_out}}"
    cargo run -p {{mobile_crate}} --bin uniffi-bindgen --release -- \
        generate --library "target/aarch64-apple-ios/release/{{mobile_lib_name}}.a" \
        --language swift \
        --out-dir "{{ios_swift_out}}"
    echo "==> Swift bindings written to {{ios_swift_out}}"

# Build sprout-mobile for all Android ABIs via cargo-ndk and copy into jniLibs
mobile-android:
    #!/usr/bin/env bash
    set -euo pipefail
    if ! command -v cargo-ndk >/dev/null 2>&1; then
        echo "error: cargo-ndk is not installed — run 'just mobile-setup' first" >&2
        exit 1
    fi
    if [ -z "${ANDROID_NDK_HOME:-}" ] && [ -z "${NDK_HOME:-}" ] && [ -z "${ANDROID_HOME:-}" ]; then
        echo "error: set ANDROID_NDK_HOME, NDK_HOME, or ANDROID_HOME" >&2
        exit 1
    fi

    echo "==> Building sprout-mobile for Android targets via cargo-ndk…"
    # cargo-ndk writes .so files directly into the jniLibs layout via -o,
    # mapping each Rust target triple to its Android ABI automatically.
    cargo ndk \
        -t arm64-v8a \
        -t armeabi-v7a \
        -t x86_64 \
        -t x86 \
        -o "{{android_jni_root}}" \
        build --release -p {{mobile_crate}}

    just mobile-kotlin-bindings
    echo "==> Android .so files copied to {{android_jni_root}}"

# Generate Kotlin UniFFI bindings from the built Rust library
mobile-kotlin-bindings:
    #!/usr/bin/env bash
    set -euo pipefail
    echo "==> Generating Kotlin bindings…"
    mkdir -p "{{android_kotlin_out}}"
    cargo run -p {{mobile_crate}} --bin uniffi-bindgen --release -- \
        generate --library "target/aarch64-linux-android/release/{{mobile_lib_name}}.so" \
        --language kotlin \
        --out-dir "{{android_kotlin_out}}"
    echo "==> Kotlin bindings written to {{android_kotlin_out}}"

# Run the sprout-mobile Rust unit tests
mobile-test:
    cargo test -p {{mobile_crate}}

# ─── Agent Harness ────────────────────────────────────────────────────────────

# Run a goose agent connected to a Sprout relay (foreground)
goose relay="ws://localhost:3000" agents="1" heartbeat="0" prompt="" key="$SPROUT_PRIVATE_KEY" token="$SPROUT_ACP_API_TOKEN":
    #!/usr/bin/env bash
    set -euo pipefail
    cargo build --release -p sprout-acp -p sprout-mcp
    env_args=(
        SPROUT_RELAY_URL="{{relay}}"
        SPROUT_PRIVATE_KEY="{{key}}"
        SPROUT_ACP_AGENT_COMMAND=goose
        SPROUT_ACP_AGENT_ARGS=acp
        SPROUT_ACP_MCP_COMMAND=./target/release/sprout-mcp-server
        SPROUT_ACP_AGENTS="{{agents}}"
        GOOSE_MODE=auto
    )
    [[ -n "{{token}}"  ]] && env_args+=(SPROUT_ACP_API_TOKEN="{{token}}")
    [[ -n "{{prompt}}" ]] && env_args+=(SPROUT_ACP_SYSTEM_PROMPT="{{prompt}}")
    if [[ "{{heartbeat}}" != "0" ]]; then
        env_args+=(SPROUT_ACP_HEARTBEAT_INTERVAL={{heartbeat}})
    fi
    exec env "${env_args[@]}" ./target/release/sprout-acp

# Run a goose agent in the background (screen session named 'goose-agent-N')
goose-bg relay="ws://localhost:3000" agents="1" heartbeat="0" prompt="" key="$SPROUT_PRIVATE_KEY" token="$SPROUT_ACP_API_TOKEN":
    #!/usr/bin/env bash
    set -euo pipefail
    cargo build --release -p sprout-acp -p sprout-mcp
    env_args=(
        SPROUT_RELAY_URL="{{relay}}"
        SPROUT_PRIVATE_KEY="{{key}}"
        SPROUT_ACP_AGENT_COMMAND=goose
        SPROUT_ACP_AGENT_ARGS=acp
        SPROUT_ACP_MCP_COMMAND=./target/release/sprout-mcp-server
        SPROUT_ACP_AGENTS="{{agents}}"
        GOOSE_MODE=auto
    )
    [[ -n "{{token}}"  ]] && env_args+=(SPROUT_ACP_API_TOKEN="{{token}}")
    [[ -n "{{prompt}}" ]] && env_args+=(SPROUT_ACP_SYSTEM_PROMPT="{{prompt}}")
    if [[ "{{heartbeat}}" != "0" ]]; then
        env_args+=(SPROUT_ACP_HEARTBEAT_INTERVAL={{heartbeat}})
    fi
    screen -dmS goose-agent-{{agents}} bash -c "$(printf '%q ' env "${env_args[@]}") ./target/release/sprout-acp"
    echo "Agent running in screen session 'goose-agent-{{agents}}'. Attach with: screen -r goose-agent-{{agents}}"
