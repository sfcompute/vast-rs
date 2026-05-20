#!/usr/bin/env bash
# generate.sh — Download the VAST VMS Swagger spec (authenticated or public).
#
# The public spec at /api/?format=openapi is a Swagger 2.0 document covering
# only the hardware/auth surface. Authenticating as a tenant admin fetches the
# full tenant-facing API (volumes, views, quotas, snapshots, users, etc.).
#
# Auth notes:
#   Cluster admins:  POST /api/token/                  { username, password }
#   Tenant admins:   POST /api/token/{tenant_name}     { username, password }
#
# Usage:
#   ./scripts/generate.sh -a host:port                      # unauthenticated (public spec)
#   ./scripts/generate.sh -a host:port -u user -p pass      # cluster admin
#   ./scripts/generate.sh -a host:port -u user -p pass -n tenant  # tenant admin
#   ./scripts/generate.sh -a host:port -t token             # pre-obtained token

set -euo pipefail

ADDRESS="${VMS_ADDRESS:-}"
USERNAME="${VMS_USER:-}"
PASSWORD="${VMS_PASSWORD:-}"
TENANT="${VMS_TENANT:-}"
TOKEN="${VMS_TOKEN:-}"
INSECURE=false

usage() {
    cat <<EOF
Usage: $(basename "$0") [OPTIONS]

Options:
  -a, --address  <host[:port]>  VMS hostname or IP (required)
  -u, --username <user>         Username (optional; fetches public spec if omitted)
  -p, --password <pass>         Password
  -n, --tenant   <name>         Tenant name — required for tenant admin accounts
  -t, --token    <token>        Pre-obtained bearer token
  -k, --insecure                Skip TLS certificate verification
  -h, --help                    Show this help message

Without credentials the public (unauthenticated) spec is fetched, which covers
only the hardware/auth surface. Provide credentials to get the full API spec.

Environment variable equivalents: VMS_ADDRESS, VMS_USER, VMS_PASSWORD, VMS_TENANT, VMS_TOKEN

Examples:
  # Public spec (hardware + auth endpoints only)
  $(basename "$0") -a vms.example.com:8443 -k

  # Full spec as tenant admin
  $(basename "$0") -a vms.example.com:8443 -u alice -p secret -n acme -k

  # Full spec as cluster admin
  $(basename "$0") -a vms.example.com:8443 -u admin -p secret -k
EOF
    exit 1
}

while [[ $# -gt 0 ]]; do
    case "$1" in
        -a|--address)  ADDRESS="$2";  shift 2 ;;
        -u|--username) USERNAME="$2"; shift 2 ;;
        -p|--password) PASSWORD="$2"; shift 2 ;;
        -n|--tenant)   TENANT="$2";   shift 2 ;;
        -t|--token)    TOKEN="$2";    shift 2 ;;
        -k|--insecure) INSECURE=true; shift ;;
        -h|--help)     usage ;;
        *) echo "Unknown option: $1" >&2; usage ;;
    esac
done

if [[ -z "$ADDRESS" ]]; then
    echo "ERROR: VMS address is required (-a / --address / VMS_ADDRESS)" >&2
    usage
fi

CURL_OPTS=(-fsSL)
if [[ "$INSECURE" == "true" ]]; then
    CURL_OPTS+=(-k)
fi

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
SPEC_DIR="$SCRIPT_DIR/../api-spec"
SPEC_FILE="$SPEC_DIR/vast-openapi.json"
mkdir -p "$SPEC_DIR"

# ---------------------------------------------------------------------------
# 1. Obtain a bearer token (if credentials were provided)
# ---------------------------------------------------------------------------
AUTH_ARGS=()

if [[ -n "$TOKEN" ]]; then
    AUTH_ARGS=(-H "Authorization: Bearer $TOKEN")

elif [[ -n "$USERNAME" && -n "$PASSWORD" ]]; then
    # Cluster admins:  POST /api/token/
    # Tenant admins:   POST /api/token/{tenant_name}
    if [[ -n "$TENANT" ]]; then
        TOKEN_URL="https://$ADDRESS/api/token/$TENANT"
        echo "Authenticating as '$USERNAME' (tenant: $TENANT) ..."
    else
        TOKEN_URL="https://$ADDRESS/api/token/"
        echo "Authenticating as '$USERNAME' ..."
    fi

    TOKEN_BODY_FILE=$(mktemp)
    HTTP_CODE=$(curl "${CURL_OPTS[@]}" \
        -X POST "$TOKEN_URL" \
        -H "Content-Type: application/json" \
        -d "{\"username\":\"$USERNAME\",\"password\":\"$PASSWORD\"}" \
        -o "$TOKEN_BODY_FILE" \
        -w "%{http_code}" 2>/dev/null)
    BODY=$(cat "$TOKEN_BODY_FILE")
    rm -f "$TOKEN_BODY_FILE"

    if [[ "$HTTP_CODE" != "200" ]]; then
        echo "ERROR: Authentication failed (HTTP $HTTP_CODE)" >&2
        echo "  Response: $BODY" >&2
        if [[ "$HTTP_CODE" == "401" && -z "$TENANT" ]]; then
            echo "  Hint: if '$USERNAME' is a tenant admin, add -n <tenant-name>" >&2
        fi
        exit 1
    fi

    TOKEN=$(echo "$BODY" | grep -o '"access":"[^"]*"' | cut -d'"' -f4)
    if [[ -z "$TOKEN" ]]; then
        echo "ERROR: Could not parse access token from response: $BODY" >&2
        exit 1
    fi

    echo "Authenticated."
    AUTH_ARGS=(-H "Authorization: Bearer $TOKEN")
else
    echo "No credentials provided — fetching public spec."
fi

# ---------------------------------------------------------------------------
# 2. Download the spec
# ---------------------------------------------------------------------------
# Try known spec endpoints — pick the largest valid JSON response.
BEST_PATHS=0
for SPEC_URL in \
    "https://$ADDRESS/api/?format=openapi" \
    "https://$ADDRESS/api/swagger.json" \
    "https://$ADDRESS/api/v1/?format=openapi" \
    "https://$ADDRESS/api/v2/?format=openapi"; do

    TMP=$(mktemp)
    HTTP_CODE=$(curl "${CURL_OPTS[@]}" "${AUTH_ARGS[@]}" "$SPEC_URL" -o "$TMP" -w "%{http_code}" 2>/dev/null) || true

    if [[ "$HTTP_CODE" == "200" ]] && python3 -c "import json,sys; json.load(open('$TMP'))" 2>/dev/null; then
        PATHS=$(python3 -c "import json; s=json.load(open('$TMP')); print(len(s.get('paths',{})))")
        echo "  $HTTP_CODE  $SPEC_URL  ($PATHS paths)"
        if [[ "$PATHS" -gt "$BEST_PATHS" ]]; then
            BEST_PATHS=$PATHS
            cp "$TMP" "$SPEC_FILE"
        fi
    else
        echo "  $HTTP_CODE  $SPEC_URL"
    fi
    rm -f "$TMP"
done

if [[ "$BEST_PATHS" -eq 0 ]]; then
    echo "ERROR: Could not retrieve a valid spec from any known endpoint." >&2
    exit 1
fi

echo ""
echo "Using spec with $BEST_PATHS paths."

MODELS=$(python3 -c "import json; s=json.load(open('$SPEC_FILE')); print(len(s.get('definitions',s.get('components',{}).get('schemas',{}))))")
echo "Saved  → $SPEC_FILE  ($BEST_PATHS paths, $MODELS models)"
echo ""
echo "Review any new or changed endpoints and update src/api/ accordingly."
