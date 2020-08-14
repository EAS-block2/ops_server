# Use the official image as a parent image.
FROM rust:1.31
WORKDIR /usr/src/ops_server
COPY config.yaml .
COPY target/release/ops_server .
RUN ip a
# general, silent, config, revere
EXPOSE 5432 5433 8082 5400

CMD [ "./ops_server" ]