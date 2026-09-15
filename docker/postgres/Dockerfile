FROM ghcr.io/tensorchord/vchord-postgres:pg18-v1.1.1

USER root
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates lsb-release wget \
    && wget -q "https://packages.groonga.org/debian/groonga-apt-source-latest-$(lsb_release --codename --short).deb" -O /tmp/groonga-apt-source.deb \
    && apt-get install -y /tmp/groonga-apt-source.deb \
    && apt-get update \
    && apt-get install -y --no-install-recommends postgresql-18-pgdg-pgroonga \
    && rm -f /tmp/groonga-apt-source.deb \
    && rm -rf /var/lib/apt/lists/*
USER postgres
