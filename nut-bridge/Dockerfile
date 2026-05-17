FROM docker.io/library/debian:trixie-slim

RUN apt-get update && \
  apt-get install -y --no-install-recommends nut-server sqlite3 ca-certificates && \
  apt-get clean autoclean && \
  apt-get autoremove --yes && \
  rm -rf /var/lib/{apt,dpkg,cache,log}/

COPY nut-bridge/ragtech-to-nut.sh /usr/local/bin/ragtech-to-nut
COPY nut-bridge/entrypoint.sh /usr/local/bin/ragtech-nut-entrypoint
RUN chmod +x /usr/local/bin/ragtech-to-nut /usr/local/bin/ragtech-nut-entrypoint

EXPOSE 3493
VOLUME /data

ENTRYPOINT ["/usr/local/bin/ragtech-nut-entrypoint"]
