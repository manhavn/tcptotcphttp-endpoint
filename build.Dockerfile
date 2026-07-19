FROM docker.io/library/rust:1.85-alpine AS build
WORKDIR /app
RUN apk add --no-cache musl-dev
ADD . .
ENV CARGO_TERM_COLOR=always
RUN cargo build --release
RUN strip target/release/tcptotcphttp-endpoint || true

FROM alpine AS runtime
COPY --from=build /app/target/release/tcptotcphttp-endpoint /tcptotcphttp-endpoint
ENTRYPOINT ["/tcptotcphttp-endpoint"]
