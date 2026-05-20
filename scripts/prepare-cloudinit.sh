#!/usr/bin/env bash
# prepare-cloudinit.sh — One-time setup to make cloud-init work under QEMU.
#
# The VAST bootstrap image uses DataSourceVMwareGuestInfo, which requires
# VMware Tools' GuestInfo API — unavailable under QEMU.  This script:
#
#   1. Patches the qcow2 to replace the VMwareGuestInfo datasource with
#      NoCloud (reads from a local ISO labelled "cidata").
#
#   2. Generates vast-seed.iso — the NoCloud seed containing:
#        meta-data  : instance identity
#        user-data  : cloud-config that installs Docker CE and runs the
#                     VAST bootstrap automatically on first boot
#
# Run this ONCE on a fresh qcow2 before the first boot.
# The qcow2 must NOT be running when you run this script.
#
# Usage:
#   ./scripts/prepare-cloudinit.sh
#   ./scripts/prepare-cloudinit.sh --qcow2 /path/to/other.qcow2
#
# Requirements (Ubuntu/Debian host):
#   sudo apt install libguestfs-tools genisoimage
#
# After running this script, run-vm.sh will detect vast-seed.iso and attach
# it automatically.  On first boot cloud-init will install Docker and then
# run the VAST bootstrap unattended.

set -euo pipefail

QCOW2="${1:-./vast-vms.qcow2}"
SEED_ISO="${SEED_ISO:-./vast-seed.iso}"
INTERFACE="${INTERFACE:-192.168.2.2}"

while [[ $# -gt 0 ]]; do
    case "$1" in
        --qcow2)     QCOW2="$2";     shift 2 ;;
        --seed-iso)  SEED_ISO="$2";  shift 2 ;;
        --interface) INTERFACE="$2"; shift 2 ;;
        --help|-h)
            grep '^#' "$0" | head -30 | sed 's/^# \{0,1\}//'
            exit 0 ;;
        *) echo "Unknown argument: $1" >&2; exit 1 ;;
    esac
done

if [[ ! -f "$QCOW2" ]]; then
    echo "ERROR: qcow2 not found: $QCOW2" >&2
    exit 1
fi

# ---------------------------------------------------------------------------
# 1. Patch the qcow2 datasource config
# ---------------------------------------------------------------------------

echo "=== Patching datasource config in $QCOW2 ==="

if ! command -v virt-customize &>/dev/null; then
    echo "ERROR: virt-customize not found." >&2
    echo "  Install with: sudo apt install libguestfs-tools" >&2
    exit 1
fi

# Replace the VMwareGuestInfo-only datasource list with NoCloud.
# "None" as a fallback makes cloud-init succeed even without the ISO attached.
sudo virt-customize -a "$QCOW2" \
    --write '/etc/cloud/cloud.cfg.d/99-DataSourceVMwareGuestInfo.cfg:datasource_list: ["NoCloud", "None"]' \
    --selinux-relabel

echo "Datasource patched."

# ---------------------------------------------------------------------------
# 2. Generate the NoCloud seed ISO
# ---------------------------------------------------------------------------

echo ""
echo "=== Generating NoCloud seed ISO: $SEED_ISO ==="

if ! command -v genisoimage &>/dev/null; then
    echo "ERROR: genisoimage not found." >&2
    echo "  Install with: sudo apt install genisoimage" >&2
    exit 1
fi

SEED_DIR=$(mktemp -d)
trap 'rm -rf "$SEED_DIR"' EXIT

# --- meta-data ---
cat > "$SEED_DIR/meta-data" <<'EOF'
instance-id: vast-dev-1
local-hostname: vast-vms
EOF

# --- user-data ---
# Installs Docker CE and runs the VAST bootstrap unattended.
# The bootstrap writes its log to /home/centos/bootstrap.log.
cat > "$SEED_DIR/user-data" <<EOF
#cloud-config

# Install Docker CE from the official repo.
# VAST VMS runs entirely as Docker containers so this is a hard prerequisite.
yum_repos:
  docker-ce-stable:
    name: Docker CE Stable
    baseurl: https://download.docker.com/linux/centos/\$releasever/\$basearch/stable
    enabled: true
    gpgcheck: true
    gpgkey: https://download.docker.com/linux/centos/gpg

packages:
  - docker-ce
  - docker-ce-cli
  - containerd.io

runcmd:
  # Start and enable Docker.
  - systemctl enable --now docker
  # Add centos to the docker group (bootstrap runs as centos).
  - usermod -aG docker centos
  # Run the VAST bootstrap unattended.
  # --interface is the management VIP the VMS will bind to.
  - >
    sudo -u centos -i
    /vast/bundles/vast_bootstrap.sh
    --skip-prompt
    --interface ${INTERFACE}
    >> /home/centos/bootstrap.log 2>&1

final_message: |
  VAST bootstrap complete.
  VMS API should be available at https://${INTERFACE}
  Default credentials: admin / 123456
  Bootstrap log: /home/centos/bootstrap.log
EOF

# Build the ISO — must be labelled "cidata" for the NoCloud datasource.
genisoimage \
    -output "$SEED_ISO" \
    -volid cidata \
    -joliet \
    -rock \
    "$SEED_DIR/user-data" \
    "$SEED_DIR/meta-data"

echo ""
echo "Seed ISO created: $SEED_ISO"
echo ""
echo "Next: run-vm.sh will attach $SEED_ISO automatically."
echo "On first boot, cloud-init will install Docker and run the VAST bootstrap."
echo "Watch progress: ssh -p 2222 centos@localhost 'tail -f bootstrap.log'"
