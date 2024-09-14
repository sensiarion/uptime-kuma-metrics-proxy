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

# our final base
FROM rust:1.79 AS final

# copy the build artifact from the build stage
COPY --from=build /app/target/release/$PROJECT_NAME .

# set the startup command to run your binary
CMD ["./api"]