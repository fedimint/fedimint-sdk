#!/usr/bin/env bash
#
# Boots the Android emulator the React Native example apps run on, creating its virtual device
# on first use. Run inside the `.#android-emulator` shell (`just android-emulator`), which
# provides the emulator, the system image and the SDK's own `adb`.
#
#   scripts/android-emulator.sh [extra emulator flags]      e.g. -no-window for a headless boot
#
# The device is `fedimint-sdk`, a Pixel 7 profile on the SDK's x86_64 Google APIs image, with
# 4 GB of RAM (the image deadlocks on memory at boot with the profile's default) and software
# rendering (the emulator's own GL path is what fails on many Linux desktops). The script
# starts the emulator, waits until Android reports the boot complete, then stays attached so
# Ctrl-C stops the device; leave it running and use `just rn-example` from another terminal.
#
# Hardware acceleration is required: on Linux the user needs read and write access to
# /dev/kvm. Without it the emulator refuses to start rather than crawling.
#
# On a host enforcing SELinux (Fedora) the emulator's qemu segfaults right after its startup
# banner, whichever build it comes from: the policy has rules for the distribution's qemu, not
# for one under /nix/store or a home directory. Confirm with `ausearch -m avc -ts recent`, then
# either build a local module (`ausearch -m avc -ts recent | audit2allow -M android-emulator`
# and `semodule -i android-emulator.pp`) or run permissive while developing.
set -euo pipefail

AVD=fedimint-sdk
IMAGE="system-images;android-36;google_apis;x86_64"
AVD_DIR="${ANDROID_AVD_HOME:-$HOME/.android/avd}/$AVD.avd"

for tool in avdmanager emulator adb; do
  command -v "$tool" >/dev/null ||
    { echo "$tool not on PATH; run in the .#android-emulator shell" >&2; exit 1; }
done
# nixpkgs' SDK exposes its tools through wrappers under the package root, one level above
# ANDROID_HOME (which points at its libexec/android-sdk); both prefixes are the shell's own.
SDK_PACKAGE="${ANDROID_HOME%/libexec/android-sdk}"
case "$(command -v adb)" in
  "${ANDROID_HOME:-/nonexistent}"/* | "${SDK_PACKAGE:-/nonexistent}"/*) ;;
  *)
    echo "adb resolves to $(command -v adb), outside the shell's SDK ($ANDROID_HOME);" \
      "another Android SDK is ahead of it on PATH, and its adb server would claim the device" >&2
    exit 1
    ;;
esac
if [[ ! -r /dev/kvm || ! -w /dev/kvm ]]; then
  echo "/dev/kvm is not accessible; the emulator needs KVM (add your user to the kvm group)" >&2
  exit 1
fi

if [[ ! -d "$AVD_DIR" ]]; then
  echo "==> Creating the $AVD virtual device"
  avdmanager create avd --name "$AVD" --device pixel_7 --package "$IMAGE" --force >/dev/null
fi
# Settings the device profile does not carry; applied on every start so an edited AVD heals.
set_ini() {
  local key=$1 value=$2 ini="$AVD_DIR/config.ini"
  if grep -q "^$key *=" "$ini"; then
    sed -i "s|^$key *=.*|$key = $value|" "$ini"
  else
    echo "$key = $value" >> "$ini"
  fi
}
set_ini hw.ramSize 4096
set_ini hw.gpu.enabled yes
set_ini hw.gpu.mode swiftshader_indirect

echo "==> Starting the emulator (device $AVD)"
emulator "@$AVD" -no-audio -no-boot-anim "$@" &
EMULATOR_PID=$!
trap 'kill "$EMULATOR_PID" 2>/dev/null || true' EXIT INT TERM

echo "==> Waiting for Android to boot"
adb wait-for-device
for _ in $(seq 1 120); do
  if [[ "$(adb shell getprop sys.boot_completed 2>/dev/null | tr -d '\r')" == "1" ]]; then
    serial=$(adb devices | awk 'NR == 2 { print $1 }')
    release=$(adb shell getprop ro.build.version.release | tr -d '\r')
    echo "==> Ready: $serial (Android $release)"
    echo "    Install an example with: just rn-example            (leave this running)"
    wait "$EMULATOR_PID"
    exit 0
  fi
  kill -0 "$EMULATOR_PID" 2>/dev/null || { echo "the emulator exited during boot" >&2; exit 1; }
  sleep 2
done
echo "Android did not finish booting within four minutes" >&2
exit 1
