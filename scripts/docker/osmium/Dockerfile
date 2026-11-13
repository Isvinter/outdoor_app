# osmium-tool for clipping and merging OSM extracts (used by scripts/build-region.sh)
FROM debian:stable-slim
RUN apt-get update \
 && apt-get install -y --no-install-recommends osmium-tool \
 && rm -rf /var/lib/apt/lists/*
ENTRYPOINT ["osmium"]
