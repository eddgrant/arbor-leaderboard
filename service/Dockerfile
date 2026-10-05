# Build on the workstation or in CI, not on Fitlet: its CPU is too slow to compile Rust comfortably.
FROM rust:1.96-bookworm AS build
WORKDIR /src
# Cache dependency builds separately from the application source.
COPY Cargo.toml Cargo.lock ./
RUN mkdir src && echo 'fn main() {}' > src/main.rs && cargo build --release && rm -rf src
COPY src ./src
RUN touch src/main.rs && cargo build --release

# Distroless "cc" provides glibc and CA certificates, which rustls' platform verifier needs.
FROM gcr.io/distroless/cc-debian12:nonroot
COPY --from=build /src/target/release/arbor-leaderboard /usr/local/bin/arbor-leaderboard
ENTRYPOINT ["/usr/local/bin/arbor-leaderboard"]
