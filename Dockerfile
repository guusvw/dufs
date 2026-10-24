FROM --platform=linux/amd64 messense/rust-musl-cross:x86_64-musl AS amd64
ARG DUFS_COMMIT
COPY . .
RUN cargo install --path . --root /

FROM --platform=linux/amd64 messense/rust-musl-cross:aarch64-musl AS arm64
ARG DUFS_COMMIT
COPY . .
RUN cargo install --path . --root /

FROM ${TARGETARCH} AS builder

FROM scratch
COPY --from=builder /bin/dufs /bin/dufs
STOPSIGNAL SIGINT
ENTRYPOINT ["/bin/dufs"]
