# Mesh LLM local build prerequisites

Sprout embeds mesh-llm through the Rust SDK pinned in Cargo. mesh-llm's native
skippy/llama layer must be available when building the relay or desktop crates.

For local development we fail closed unless the native libraries are already
prepared. Build them once from the pinned mesh-llm checkout, then point Cargo at
the library directory:

```bash
# From a mesh-llm checkout pinned to the same rev as Cargo.toml (bd16da4 for v1):
scripts/prepare-llama.sh pinned
LLAMA_STAGE_LINK_MODE=dynamic scripts/build-llama.sh

# Then build Sprout with dynamic linking enabled. Adjust the lib dir to the
# build output printed by mesh-llm's build script.
export LLAMA_STAGE_LINK_MODE=dynamic
export LLAMA_STAGE_LIB_DIR=/path/to/mesh-llm/.deps/llama-build/build-stage-abi-cpu/lib
cargo check --manifest-path desktop/src-tauri/Cargo.toml
cargo check -p sprout-relay
```

If `LLAMA_STAGE_LINK_MODE` is omitted, mesh-llm's build script may try to build
or link static llama archives. CI will eventually cache that build, but the first
local milestone expects explicit dynamic-link configuration so missing native
artifacts fail with a clear build error instead of silently doing surprise native
work.
