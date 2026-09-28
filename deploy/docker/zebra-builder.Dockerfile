# Fallback: compile the Zebra Store backend inside Docker (no zig/rust needed on the host).
# Slower than scripts/build.sh (cargo-zigbuild) and keeps a ~3 GB cache in the Docker VM.
#   docker buildx build --platform linux/amd64 -f deploy/docker/zebra-builder.Dockerfile \
#       -t zebra-lab/zebra:local --load .          # context = repo root
FROM --platform=$BUILDPLATFORM rust:1-bookworm AS build
ARG TARGETARCH
RUN apt-get update && apt-get install -y --no-install-recommends musl-tools python3-pip \
    && pip install --break-system-packages ziglang cargo-zigbuild \
    && rustup target add x86_64-unknown-linux-musl aarch64-unknown-linux-musl
WORKDIR /src
COPY backend /src/backend
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/src/backend/target \
    cd backend && case "$TARGETARCH" in amd64) T=x86_64-unknown-linux-musl;; arm64) T=aarch64-unknown-linux-musl;; esac \
    && cargo zigbuild --release -p zs-server --target $T \
    && cp target/$T/release/zebra-store /zebra-store

FROM alpine:3.20
RUN apk add --no-cache ca-certificates tzdata \
    && addgroup -S zebra && adduser -S -G zebra -h /app zebra \
    && mkdir -p /app/data /app/uploads && chown -R zebra:zebra /app
COPY --from=build /zebra-store /usr/local/bin/zebra-store
WORKDIR /app
USER zebra
VOLUME ["/app/data", "/app/uploads"]
EXPOSE 8081
ENTRYPOINT ["/usr/local/bin/zebra-store", "--config", "/app/config.yml"]
CMD ["serve"]
