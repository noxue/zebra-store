# Zebra Store backend runtime image.
#
# The binary is cross-compiled on the build machine (scripts/build.sh, cargo-zigbuild,
# static musl) and copied in per target arch, so building the linux/amd64 image on an
# arm64 Mac needs no emulation for the Rust compile. The SPAs are served by the edge
# Caddy image, not by this one.
#
# Fallback without zig: docker/zebra-builder.Dockerfile compiles inside Docker.
FROM alpine:3.20
ARG TARGETARCH
RUN apk add --no-cache ca-certificates tzdata \
    && addgroup -S zebra && adduser -S -G zebra -h /app zebra \
    && mkdir -p /app/data /app/uploads && chown -R zebra:zebra /app
COPY --chmod=0755 build/bin/zebra-store-${TARGETARCH} /usr/local/bin/zebra-store
WORKDIR /app
USER zebra
VOLUME ["/app/data", "/app/uploads"]
EXPOSE 8081
HEALTHCHECK --interval=10s --timeout=3s --start-period=10s --retries=6 \
    CMD wget -q -O /dev/null http://127.0.0.1:8081/api/v1/public/config || exit 1
ENTRYPOINT ["/usr/local/bin/zebra-store", "--config", "/app/config.yml"]
CMD ["serve"]
