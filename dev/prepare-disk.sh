#!/usr/bin/env bash
# prepare-disk.sh — Convert VAST OVA/VMDK to a QEMU-ready qcow2 image.
#
# Run this once on the Linux host before run-vm.sh.
#
# Steps:
#   1. Extract the VMDK from the OVA (if not already done)
#   2. Convert VMDK → qcow2
#   3. Verify the image and print its info
#   4. (Optional) Resize the image if you want more headroom
#
# Usage:
#   ./scripts/prepare-disk.sh --ova /path/to/vast.ova
#   ./scripts/prepare-disk.sh --vmdk /path/to/disk.vmdk   # skip extraction
#   ./scripts/prepare-disk.sh --check /path/to/vast-vms.qcow2  # verify existing

set -euo pipefail

OVA=""
VMDK=""
CHECK_ONLY=""
OUTPUT="${OUTPUT:-$(dirname "$0")/../vast-vms.qcow2}"

# ---------------------------------------------------------------------------
# Argument parsing
# ---------------------------------------------------------------------------

while [[ $# -gt 0 ]]; do
    case "$1" in
        --ova)        OVA="$2";        shift 2 ;;
        --vmdk)       VMDK="$2";       shift 2 ;;
        --check)      CHECK_ONLY="$2"; shift 2 ;;
        --output|-o)  OUTPUT="$2";     shift 2 ;;
        --help|-h)
            grep '^#' "$0" | sed 's/^# \{0,1\}//' | head -20
            exit 0 ;;
        *) echo "Unknown argument: $1" >&2; exit 1 ;;
    esac
done

# ---------------------------------------------------------------------------
# Check-only mode — verify an existing image
# ---------------------------------------------------------------------------

if [[ -n "$CHECK_ONLY" ]]; then
    echo "=== Image info ==="
    qemu-img info "$CHECK_ONLY"
    echo ""
    echo "=== Consistency check ==="
    qemu-img check "$CHECK_ONLY" && echo "Image OK."
    exit 0
fi

# ---------------------------------------------------------------------------
# Require qemu-img
# ---------------------------------------------------------------------------

if ! command -v qemu-img &>/dev/null; then
    echo "ERROR: qemu-img not found." >&2
    echo "  Install with: sudo dnf install qemu-img   (Rocky/RHEL)" >&2
    echo "            or: sudo apt install qemu-utils  (Debian/Ubuntu)" >&2
    exit 1
fi

# ---------------------------------------------------------------------------
# Step 1: Extract VMDK from OVA (if --ova was given)
# ---------------------------------------------------------------------------

if [[ -n "$OVA" ]]; then
    if [[ ! -f "$OVA" ]]; then
        echo "ERROR: OVA not found: $OVA" >&2
        exit 1
    fi

    echo "=== Extracting OVA: $OVA ==="
    EXTRACT_DIR="$(dirname "$OVA")"
    tar xf "$OVA" -C "$EXTRACT_DIR" --verbose

    # Find the VMDK — OVA contains exactly one.
    VMDK=$(find "$EXTRACT_DIR" -name "*.vmdk" | head -1)
    if [[ -z "$VMDK" ]]; then
        echo "ERROR: No VMDK found after extracting OVA." >&2
        exit 1
    fi
    echo "Found VMDK: $VMDK"
fi

# ---------------------------------------------------------------------------
# Step 2: Convert VMDK → qcow2
# ---------------------------------------------------------------------------

if [[ -z "$VMDK" ]]; then
    echo "ERROR: Provide --ova or --vmdk." >&2
    exit 1
fi

if [[ ! -f "$VMDK" ]]; then
    echo "ERROR: VMDK not found: $VMDK" >&2
    exit 1
fi

echo ""
echo "=== Converting VMDK → qcow2 ==="
echo "  Input : $VMDK"
echo "  Output: $OUTPUT"
echo ""

# -p   show progress
# -W   write in parallel for speed (qemu-img 5.0+, safe for new images)
# -c   enable qcow2 compression (saves disk space at slight CPU cost)
# Omit -c if you want maximum I/O performance at the cost of image size.
qemu-img convert \
    -f vmdk \
    -O qcow2 \
    -p \
    "$VMDK" \
    "$OUTPUT"

# ---------------------------------------------------------------------------
# Step 3: Verify
# ---------------------------------------------------------------------------

echo ""
echo "=== Image info ==="
qemu-img info "$OUTPUT"

echo ""
echo "=== Consistency check ==="
qemu-img check "$OUTPUT" && echo "Image OK."

# ---------------------------------------------------------------------------
# Step 4: Summary
# ---------------------------------------------------------------------------

echo ""
echo "Boot disk ready: $OUTPUT"
echo ""
echo "Next steps:"
echo "  1. Identify your VAST storage disk (the additional ~1 TB drive):"
echo "       lsblk -o NAME,SIZE,TYPE,MOUNTPOINT"
echo "  2. Set DATA_DISK in scripts/run-vm.sh (default: /dev/sdb)"
echo "  3. Launch the VM:"
echo "       ./scripts/run-vm.sh"
echo "  4. Watch the console on first boot:"
echo "       socat -,raw,echo=0 UNIX-CONNECT:/tmp/vast-vm-console.sock"
echo "  5. Note the IP the VMS assigns itself, or use localhost port-forwards."
