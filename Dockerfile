FROM rust:latest
WORKDIR /usr/src/ops_server
COPY . .
RUN ping -c 4 google.com
RUN cargo install --path .
EXPOSE 5432 5433 6432 6433 8082
CMD ["ops_server"]