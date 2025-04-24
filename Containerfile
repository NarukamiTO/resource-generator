FROM rust:1.86 as builder
WORKDIR /app

# Copy manifest first to cache crates.io dependencies
COPY Cargo.toml Cargo.lock ./
RUN mkdir src
RUN echo "// dummy" > src/lib.rs
RUN cargo fetch

# Copy real sources and build
COPY src ./src
RUN cargo build --release

# Stage 2: runtime image
FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates git \
 && rm -rf /var/lib/apt/lists/*

# Copy the built binary
COPY --from=builder /app/target/release/resource-generator /usr/local/bin/resource-generator

WORKDIR /app
VOLUME ["/app/resources", "/app/out"]

# On start, clone (or update) the resources repo and then run the generator
ENTRYPOINT [ "sh", "-c", \
    "if [ ! -d resources/.git ]; then \
        git clone --depth 1 https://github.com/NarukamiTO/resources.git resources; \
     else \
        cd resources && git pull --ff-only && cd ..; \
     fi && \
     RUST_LOG=info resource-generator" \
]
