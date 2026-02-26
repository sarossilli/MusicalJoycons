#!/usr/bin/env bash
set -euo pipefail

DEVICE_NAMES=("Joy-Con (L)" "Joy-Con (R)" "Pro Controller")
SCAN_TIMEOUT="${1:-30}"

cleanup() {
    echo "Stopping scan..."
    bluetoothctl scan off &>/dev/null || true
}
trap cleanup EXIT

ensure_kernel_modules() {
    if ! lsmod | grep -q btusb 2>/dev/null; then
        echo "Loading Bluetooth kernel modules..."
        modprobe bluetooth 2>/dev/null || true
        modprobe btusb 2>/dev/null || true
        sleep 1
    fi
}

ensure_daemons() {
    if ! pgrep -x dbus-daemon &>/dev/null; then
        echo "Starting D-Bus daemon..."
        mkdir -p /run/dbus
        rm -f /run/dbus/pid
        dbus-daemon --system 2>/dev/null || true
        sleep 1
    fi

    if ! pgrep -x bluetoothd &>/dev/null; then
        echo "Starting Bluetooth daemon..."
        /usr/libexec/bluetooth/bluetoothd &>/dev/null &
        sleep 2
    fi
}

check_prerequisites() {
    if ! command -v bluetoothctl &>/dev/null; then
        echo "Error: bluetoothctl not found. Install bluez package."
        exit 1
    fi

    ensure_kernel_modules
    ensure_daemons

    if ! bluetoothctl show &>/dev/null; then
        echo "Error: No Bluetooth adapter found."
        echo "If running in a container, ensure the host Bluetooth adapter is forwarded."
        echo "  WSL2: usbipd bind --busid <ID> && usbipd attach --wsl --busid <ID>"
        exit 1
    fi
}

start_bluetooth() {
    echo "Powering on Bluetooth adapter..."
    bluetoothctl power on
    bluetoothctl agent on &>/dev/null || true
    bluetoothctl default-agent &>/dev/null || true
}

is_target_device() {
    local name="$1"
    for target in "${DEVICE_NAMES[@]}"; do
        if [[ "$name" == *"$target"* ]]; then
            return 0
        fi
    done
    return 1
}

scan_and_pair() {
    echo ""
    echo "Scanning for Nintendo controllers (${SCAN_TIMEOUT}s)..."
    echo "Hold the SYNC button on your Joy-Con or Pro Controller now."
    echo ""

    bluetoothctl scan on &>/dev/null &
    local scan_pid=$!

    local found=()
    local elapsed=0

    while (( elapsed < SCAN_TIMEOUT )); do
        sleep 2
        elapsed=$((elapsed + 2))

        while IFS= read -r line; do
            local mac name
            mac=$(echo "$line" | awk '{print $2}')
            name=$(echo "$line" | cut -d' ' -f3-)

            if is_target_device "$name"; then
                if [[ ! " ${found[*]:-} " =~ " ${mac} " ]]; then
                    found+=("$mac")
                    echo "Found: $name ($mac)"
                    pair_device "$mac" "$name"
                fi
            fi
        done < <(bluetoothctl devices 2>/dev/null | grep -E "Joy-Con|Pro Controller" || true)

        printf "\r  %d/%ds elapsed, %d device(s) found..." "$elapsed" "$SCAN_TIMEOUT" "${#found[@]}"
    done

    kill "$scan_pid" 2>/dev/null || true
    echo ""

    if [[ ${#found[@]} -eq 0 ]]; then
        echo ""
        echo "No Nintendo controllers found."
        echo ""
        echo "Troubleshooting:"
        echo "  1. Hold the small SYNC button (not a shoulder button) until LEDs flash"
        echo "  2. Check that the Bluetooth adapter is visible: bluetoothctl show"
        echo "  3. Make sure the controller isn't paired to another device"
        exit 1
    fi

    echo ""
    echo "Done! ${#found[@]} device(s) paired."
    bluetoothctl devices
}

pair_device() {
    local mac="$1"
    local name="$2"

    echo "  Pairing $name ($mac)..."
    if bluetoothctl pair "$mac" 2>/dev/null; then
        bluetoothctl trust "$mac" 2>/dev/null
        bluetoothctl connect "$mac" 2>/dev/null
        echo "  Paired and connected: $name"
    else
        echo "  Warning: Failed to pair $name ($mac) — it may already be paired"
        bluetoothctl trust "$mac" 2>/dev/null || true
        bluetoothctl connect "$mac" 2>/dev/null || true
    fi
}

list_paired() {
    echo ""
    echo "Currently paired Nintendo controllers:"
    local count=0
    while IFS= read -r line; do
        local name
        name=$(echo "$line" | cut -d' ' -f3-)
        if is_target_device "$name"; then
            echo "  $line"
            count=$((count + 1))
        fi
    done < <(bluetoothctl devices Paired 2>/dev/null || bluetoothctl paired-devices 2>/dev/null || true)

    if [[ $count -eq 0 ]]; then
        echo "  (none)"
    fi
}

main() {
    echo "=== Joy-Con Bluetooth Pairing ==="
    echo ""

    check_prerequisites
    start_bluetooth
    list_paired
    scan_and_pair
}

main
