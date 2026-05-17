FROM docker.io/library/debian:trixie-slim

RUN apt-get update && \
  apt-get install -y --no-install-recommends nut-server sqlite3 ca-certificates && \
  apt-get clean autoclean && \
  apt-get autoremove --yes && \
  rm -rf /var/lib/{apt,dpkg,cache,log}/

COPY nut-bridge/ragtech-to-nut.sh /usr/local/bin/ragtech-to-nut
COPY nut-bridge/entrypoint.sh /usr/local/bin/ragtech-nut-entrypoint
COPY nut-bridge/healthcheck.sh /usr/local/bin/ragtech-nut-healthcheck
RUN chmod +x /usr/local/bin/ragtech-to-nut /usr/local/bin/ragtech-nut-entrypoint /usr/local/bin/ragtech-nut-healthcheck

EXPOSE 3493
VOLUME /data
HEALTHCHECK --interval=30s --timeout=5s --start-period=30s --retries=3 \
  CMD ["/usr/local/bin/ragtech-nut-healthcheck"]

ENTRYPOINT ["/usr/local/bin/ragtech-nut-entrypoint"]
