# Remit development recipes

# Run all checks: fmt, clippy, tests, swift build
check: fmt-check clippy test swift-build

# Check formatting
fmt-check:
    cargo fmt --all -- --check

# Apply formatting
fmt:
    cargo fmt --all

# Run clippy lints
clippy:
    cargo clippy --workspace --all-targets -- -D warnings

# Run tests
test:
    cargo test --workspace

# Build all crates
build:
    cargo build --workspace

# Build in release mode
build-release:
    cargo build --workspace --release

# Build the Swift prompter binary (skipped if `swift` toolchain absent)
swift-build:
    @command -v swift >/dev/null && (cd swift && swift build) || echo "swift toolchain absent; skipping"

# Run Swift unit tests (skipped if `swift` toolchain absent)
swift-test:
    @command -v swift >/dev/null && (cd swift && swift test) || echo "swift toolchain absent; skipping"
