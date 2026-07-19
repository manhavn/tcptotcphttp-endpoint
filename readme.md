# tcptotcphttp-endpoint

Rust TCP reverse-tunnel **endpoint agent** with **HTTP control plane** (no UDP).

Pairs with [tcptotcphttp-server](../tcptotcphttp-server) or [tcptotcpgohttp-server](../tcptotcpgohttp-server).

## Env / CLI

| Flag | ENV | Default |
|------|-----|---------|
| `--server-host` | `RUST_APP_SERVER_HOST` | required |
| `--server-port` | `RUST_APP_SERVER_PORT` | `3000` |
| `--endpoint` | `RUST_APP_ADDR_ENDPOINT` | required (e.g. `127.0.0.1:22`) |
| `--register` | `RUST_APP_REGISTER_VALUE` | `preferred_client\|preferred_app\|token` |
| `--poll-mode` | `RUST_APP_POLL_MODE` | `long` |
| `--poll-wait` | `RUST_APP_POLL_WAIT` | `20` |
| `--local-http` | `RUST_APP_LOCAL_HTTP_PORT` | `0` (disabled unless > 0) |
| `--env-file` | `RUST_APP_FILE_PATH_ENV_APP` | `env/app.env` |

```bash
cargo run --release -- \
  --server-host 127.0.0.1 \
  --server-port 3000 \
  --endpoint 127.0.0.1:22 \
  --register '55555|0|acbdef123456'

./scripts/build-linux-amd64.sh
./tcptotcphttp-endpoint-linux-amd64 \
  --server-host tcp.example.com \
  --endpoint 127.0.0.1:22 \
  --register '55555|0|token'
```

## Flow

1. `POST /v1/register` → `key`, `client_port`, `app_port`
2. Long-poll `GET /v1/pending`
3. For each pending count: dial `server:app_port` + local endpoint, `tcptotcp::connect`
4. On exit: `POST /v1/quit` with full key
