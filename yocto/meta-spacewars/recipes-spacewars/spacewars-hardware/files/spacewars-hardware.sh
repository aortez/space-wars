#!/bin/sh
# Read a data-only boot profile; never source shell code from the boot partition.
set -eu

boot_dir=${SPACEWARS_BOOT_DIR:-/boot}
run_dir=${SPACEWARS_HARDWARE_RUN_DIR:-/run/spacewars-hardware}
sys_dir=${SPACEWARS_SYS_DIR:-/sys}
profile_file="$boot_dir/spacewars-device.txt"

if [ ! -f "$profile_file" ]; then
    echo "Missing $profile_file; flash a unified Pi 4/5 image first." >&2
    exit 1
fi
if sed 's/\r$//' "$profile_file" | grep -Eqv '^(#.*|\[all\]|include spacewars-profiles/(hdmi|picade|hyperpixel)\.txt|[[:space:]]*)$'; then
    echo "Invalid hardware selection in $profile_file; use only one profile include under [all]." >&2
    exit 1
fi
profile=$(sed -n 's/^include spacewars-profiles\/\([a-z0-9-]*\)\.txt\r\{0,1\}$/\1/p' "$profile_file")
case "$profile" in
    hdmi|picade|hyperpixel) ;;
    *) echo "Invalid or ambiguous hardware profile in $profile_file" >&2; exit 1 ;;
esac

if [ "$profile" = picade ]; then
    model_file="$sys_dir/firmware/devicetree/base/model"
    model=$(tr -d '\000' < "$model_file")
    case "$model" in
        'Raspberry Pi 4 Model B'*) ;;
        *) echo "Picade X HAT profile currently targets Pi 4 Model B, not $model" >&2; exit 1 ;;
    esac
fi

mkdir -p "$run_dir"
output="$run_dir/hardware.env"
temporary=$(mktemp "$run_dir/hardware.env.XXXXXX")
trap 'rm -f "$temporary"' EXIT HUP INT TERM

{
    printf 'SPACEWARS_HARDWARE_PROFILE=%s\n' "$profile"
    if [ "$profile" = hyperpixel ]; then
        printf 'SLINT_DRM_OUTPUT=DPI-1\nSLINT_KMS_ROTATION=90\nALSA_CARD=Audio\n'
    else
        printf 'SLINT_KMS_ROTATION=0\n'
        connector_name=""
        for connector in "$sys_dir"/class/drm/card*-HDMI-A-*; do
            [ -f "$connector/status" ] || continue
            [ "$(cat "$connector/status")" = connected ] || continue
            connector_name=${connector##*/}
            connector_name=${connector_name#card*-}
            printf 'SLINT_DRM_OUTPUT=%s\n' "$connector_name"
            break
        done
        if [ "$profile" = picade ]; then
            # Stable ALSA card ID from the HAT's hifiberry-dac driver, not card 0.
            printf 'ALSA_CARD=sndrpihifiberry\n'
        elif [ "$connector_name" = HDMI-A-1 ]; then
            printf 'ALSA_CARD=vc4hdmi0\n'
        elif [ "$connector_name" = HDMI-A-2 ]; then
            printf 'ALSA_CARD=vc4hdmi1\n'
        fi
    fi
} > "$temporary"
chmod 644 "$temporary"
mv "$temporary" "$output"

if [ "$profile" = hyperpixel ]; then
    # Preserve the existing kiosk's backlight workaround, but only for its profile.
    attempts=0
    while [ "$attempts" -lt 50 ]; do
        backlight="$sys_dir/class/backlight/backlight"
        if [ -e "$backlight/bl_power" ]; then
            printf '0\n' > "$backlight/bl_power"
            printf '1\n' > "$backlight/brightness"
            break
        fi
        attempts=$((attempts + 1))
        sleep 0.1
    done
fi
echo "Space-Wars hardware profile: $profile ($output)"
