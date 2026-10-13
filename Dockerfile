FROM rust:1.79 AS build

ENV PROJECT_NAME=api

# create a new empty shell project
RUN USER=root cargo new --bin $PROJECT_NAME
WORKDIR /app

# copy over your manifests
COPY ./Cargo.lock ./Cargo.lock
COPY ./Cargo.toml ./Cargo.toml


# copy your source tree
COPY ./src ./src
RUN cargo build --release

# our final base: glibc + libgcc + CA certs only. Must match the glibc of the
# build stage (rust:1.79 is bookworm), so no alpine/musl here; openssl is vendored.
FROM gcr.io/distroless/cc-debian12 AS final

# copy only the binary (ENV from the build stage is not visible here)
COPY --from=build /app/target/release/api /api

# set the startup command to run your binary
CMD ["/api"]