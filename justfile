set dotenv-load := true

dev:
    cargo watch -x run

uidev:
    cd ui && bun run dev

create-migration name:
    sqlx migrate add {{name}}

reset-db:
    sqlx database reset

# DOCKER_DEFAULT_PLATFORM forces the amd64 image on Apple Silicon (runs under emulation).
build:
    DOCKER_DEFAULT_PLATFORM=linux/amd64 cross build --release --target=x86_64-unknown-linux-gnu
    cd ui && bun install && bun run build

package: build
    plugin-cli package
