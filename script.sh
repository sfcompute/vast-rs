#!/bin/bash

set -euf -o pipefail

for PATH in "?format=openapi" "swagger.json" "schema/" "openapi.json"; do
  echo $(which curl)
  CODE=$(curl -sk -o /tmp/vast-spec-test.json -w "%{http_code}" \
    "https://munin-proxy.bulk.northerndata.tech:8443/api/$PATH")
  echo "$CODE  /api/$PATH  ($(wc -c < /tmp/vast-spec-test.json) bytes)"
done
