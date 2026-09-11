ARG DEBIAN_IMAGE=docker.io/library/debian:trixie-slim@sha256:109e2c65005bf160609e4ba6acf7783752f8502ad218e298253428690b9eaa4b
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

FROM ${DEBIAN_IMAGE}
ARG NUT_VERSION

RUN apt-get update && \
  apt-get install -y --no-install-recommends adduser bash ca-certificates sqlite3 && \
  apt-get clean autoclean && \
  apt-get autoremove --yes && \
  rm -rf /var/lib/{apt,dpkg,cache,log}/ && \
  addgroup --system nut && \
  adduser --system --ingroup nut --no-create-home --home /var/lib/nut nut

COPY --from=nut-builder /opt/nut-install/ /
RUN ldconfig && \
  test "$(upsd -V 2>&1)" = "Network UPS Tools upsd ${NUT_VERSION} release"

COPY nut-bridge/ragtech-to-nut.sh /usr/local/bin/ragtech-to-nut
COPY nut-bridge/entrypoint.sh /usr/local/bin/ragtech-nut-entrypoint
COPY nut-bridge/healthcheck.sh /usr/local/bin/ragtech-nut-healthcheck
RUN chmod +x /usr/local/bin/ragtech-to-nut /usr/local/bin/ragtech-nut-entrypoint /usr/local/bin/ragtech-nut-healthcheck

EXPOSE 3493
VOLUME /data
HEALTHCHECK --interval=30s --timeout=5s --start-period=30s --retries=3 \
  CMD ["/usr/local/bin/ragtech-nut-healthcheck"]

ENTRYPOINT ["/usr/local/bin/ragtech-nut-entrypoint"]
