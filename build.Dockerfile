FROM docker.io/library/rust:1.85-alpine AS build
WORKDIR /app
RUN apk add --no-cache musl-dev
ADD . .
ENV CARGO_TERM_COLOR=always
RUN rustup target add x86_64-unknown-linux-musl
RUN cargo build --release --target x86_64-unknown-linux-musl
RUN strip target/x86_64-unknown-linux-musl/release/tcptotcphttp-endpoint || true

FROM alpine AS runtime
COPY --from=build /app/target/x86_64-unknown-linux-musl/release/tcptotcphttp-endpoint /tcptotcphttp-endpoint
ENTRYPOINT ["/tcptotcphttp-endpoint"]
