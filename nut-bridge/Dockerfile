ARG DEBIAN_IMAGE=docker.io/library/debian:trixie-slim@sha256:109e2c65005bf160609e4ba6acf7783752f8502ad218e298253428690b9eaa4b
ARG RUST_IMAGE=docker.io/library/rust:1.98.1-slim-trixie@sha256:4cd829461bd5c4d511c32e269da9cb8929223b666519d8004e35fc8d1d771ab7
ARG NUT_VERSION=2.8.5
ARG NUT_SHA256=18bf32e59eb764b13da3c4fa70384926d7fa584cb31d2fe7f137a570633eeec1

FROM ${DEBIAN_IMAGE} AS nut-builder
ARG NUT_VERSION
ARG NUT_SHA256

RUN apt-get update && \
  apt-get install -y --no-install-recommends build-essential ca-certificates curl pkg-config && \
  apt-get clean autoclean && \
  apt-get autoremove --yes && \
  rm -rf /var/lib/{apt,dpkg,cache,log}/

WORKDIR /tmp/nut
RUN curl -fsSLo /tmp/nut.tar.gz "https://networkupstools.org/source/2.8/nut-${NUT_VERSION}.tar.gz" && \
  echo "${NUT_SHA256}  /tmp/nut.tar.gz" | sha256sum -c - && \
  tar -xzf /tmp/nut.tar.gz --strip-components=1 && \
  ./configure \
    --prefix=/usr/local \
    --sysconfdir=/etc/nut \
    --with-user=nut \
    --with-group=nut \
    --with-statepath=/run/nut \
    --with-altpidpath=/run/nut \
    --with-drivers=dummy-ups \
    --with-docs=no \
    --with-cgi=no \
    --with-dev=no && \
  make -j2 && \
  make DESTDIR=/opt/nut-install install

# The exporter is built natively for each target platform; SQLite is compiled
# in (rusqlite "bundled"), so the runtime image needs no sqlite3 packages.
FROM ${RUST_IMAGE} AS exporter-builder

WORKDIR /src
COPY Cargo.toml Cargo.lock ./
COPY nut-bridge/Cargo.toml nut-bridge/Cargo.toml
COPY nut-bridge/src nut-bridge/src
# No RUN --mount cache here: the legacy Docker builder (Debian's docker-cli,
# used by tests/run-in-container.sh) rejects it; layer caching still applies.
RUN cargo build --release --locked --package ragtech-nut-bridge --bin ragtech-to-nut && \
  install -D -m 0755 target/release/ragtech-to-nut /out/ragtech-to-nut

FROM ${DEBIAN_IMAGE}
ARG NUT_VERSION

RUN apt-get update && \
  apt-get install -y --no-install-recommends adduser bash ca-certificates && \
  apt-get clean autoclean && \
  apt-get autoremove --yes && \
  rm -rf /var/lib/{apt,dpkg,cache,log}/ && \
  addgroup --system nut && \
  adduser --system --ingroup nut --no-create-home --home /var/lib/nut nut

COPY --from=nut-builder /opt/nut-install/ /
RUN ldconfig && \
  test "$(upsd -V 2>&1)" = "Network UPS Tools upsd ${NUT_VERSION} release"

COPY --from=exporter-builder /out/ragtech-to-nut /usr/local/bin/ragtech-to-nut
COPY nut-bridge/entrypoint.sh /usr/local/bin/ragtech-nut-entrypoint
COPY nut-bridge/healthcheck.sh /usr/local/bin/ragtech-nut-healthcheck
RUN chmod +x /usr/local/bin/ragtech-nut-entrypoint /usr/local/bin/ragtech-nut-healthcheck

EXPOSE 3493
VOLUME /data
HEALTHCHECK --interval=30s --timeout=5s --start-period=30s --retries=3 \
  CMD ["/usr/local/bin/ragtech-nut-healthcheck"]

ENTRYPOINT ["/usr/local/bin/ragtech-nut-entrypoint"]
