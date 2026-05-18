FROM docker.io/library/debian:trixie-slim@sha256:109e2c65005bf160609e4ba6acf7783752f8502ad218e298253428690b9eaa4b

RUN apt-get update && \
  apt-get install -y --no-install-recommends \
    bash \
    bats \
    ca-certificates \
    coreutils \
    docker-cli \
    findutils \
    nut-client \
    procps \
    shellcheck \
    sqlite3 && \
  apt-get clean autoclean && \
  apt-get autoremove --yes && \
  rm -rf /var/lib/{apt,dpkg,cache,log}/

WORKDIR /work
