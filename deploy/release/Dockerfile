FROM alpine:3.22 AS download

ARG REPOSITORY=noxue/zebra-store
ARG RELEASE=latest
ARG TARGET=x86_64

RUN apk add --no-cache ca-certificates curl tar
WORKDIR /download
RUN asset="zebra-store-linux-${TARGET}-musl.tar.gz" \
    && if [ "$RELEASE" = latest ]; then \
      base="https://github.com/${REPOSITORY}/releases/latest/download"; \
    else \
      base="https://github.com/${REPOSITORY}/releases/download/${RELEASE}"; \
    fi \
    && curl -fL --retry 3 "$base/$asset" -o "$asset" \
    && curl -fL --retry 3 "$base/SHA256SUMS" -o SHA256SUMS \
    && grep "  $asset\$" SHA256SUMS | sha256sum -c - \
    && tar -xzf "$asset"

FROM alpine:3.22

RUN apk add --no-cache ca-certificates tzdata wget \
    && mkdir -p /app/data /app/uploads \
    && chown -R 100:101 /app
COPY --from=download /download/zebra-store /usr/local/bin/zebra-store

WORKDIR /app
USER 100:101
VOLUME ["/app/data", "/app/uploads"]
EXPOSE 8081
HEALTHCHECK --interval=10s --timeout=5s --start-period=20s --retries=12 \
  CMD wget -qO- http://127.0.0.1:8081/health >/dev/null || exit 1
ENTRYPOINT ["/usr/local/bin/zebra-store", "--config", "/app/config.yml"]
CMD ["serve"]
