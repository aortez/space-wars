#!/bin/bash
# Temporary production-KMS diagnostic. No installed binaries/settings are replaced.
set -euo pipefail
if [[ $EUID != 0 || $# != 8 ]]; then
  echo "Usage (as administrator): $0 /tmp/spacewars-pizza-NAME RUN BALLS LIMIT STEPS off|on mixed|ordinary INTERVALS" >&2
  exit 2
fi
root=$1
name=$2
balls=$3
limit=$4
steps=$5
field=$6
kind=$7
intervals=$8
out="$root/$name"
[[ "$root" == /tmp/spacewars-pizza-* && "$name" =~ ^[a-zA-Z0-9-]+$ && "$intervals" =~ ^[0-9]+$ ]]
[[ "$(readlink -f "$root")" == "$root" ]]
[[ "$balls" =~ ^[0-9]+$ && "$limit" =~ ^[0-9]+$ && "$steps" =~ ^[0-9]+$ ]]
((balls >= 64 && balls <= limit && limit <= 5000 && steps >= 1 && steps <= 3600 && intervals >= 1 && intervals <= 600))
[[ "$field" == off || "$field" == on ]]
[[ "$kind" == mixed || "$kind" == ordinary ]]
[[ ! -e "$out" ]]
if systemctl is-active --quiet spacewars-pizza-lab.service; then
  echo "Another Pizza capture is active" >&2
  exit 1
fi
sudo -n install -d -o spacewars -g spacewars -m 0750 "$out" "$out/config"
cli=(sudo -n -u spacewars -- "$root/spacewars-cli")
socket="$out/control.sock"
was_active=$(systemctl is-active spacewars-kiosk.service || true)
cleanup() {
  sudo -n systemctl stop spacewars-pizza-lab.service || true
  sudo -n journalctl -u spacewars-pizza-lab.service -n 150 --no-pager > "$out/journal.txt" || true
  if [[ "$was_active" == active ]]; then sudo -n systemctl start spacewars-kiosk.service; fi
}
trap cleanup EXIT
trap 'exit 130' HUP INT TERM
sudo -n systemctl stop spacewars-kiosk.service
args=(--scenario pizza --seed 7 --fullscreen --renderer raster --raster-scale 1 --config-dir "$out/config" --pizza-lab-balls "$balls" --pizza-lab-limit "$limit" --pizza-lab-steps "$steps" --pizza-lab-report "$out/simulation.json")
[[ "$field" == on ]] && args+=(--pizza-lab-field)
[[ "$kind" == ordinary ]] && args+=(--pizza-lab-ordinary)
printf '%q ' "$root/engine-client" "${args[@]}" > "$out/command.txt"
uname -a > "$out/device.txt"
cat /proc/device-tree/model >> "$out/device.txt"
cat /sys/devices/system/cpu/cpu0/cpufreq/scaling_governor >> "$out/device.txt"
cat /usr/share/spacewars/build.json > "$out/installed-image-build.json"
sha256sum "$root/engine-client" "$root/spacewars-cli" > "$out/binary.sha256"
sudo -n systemd-run --unit spacewars-pizza-lab --collect --service-type=exec \
  -p User=spacewars -p Group=spacewars -p 'SupplementaryGroups=input video audio' \
  -p WorkingDirectory=/data/spacewars -p NoNewPrivileges=yes -p "RuntimeMaxSec=$((intervals+60))" \
  -p EnvironmentFile=/run/spacewars-hardware/hardware.env \
  -p EnvironmentFile=-/boot/spacewars-kiosk.env \
  --setenv=SLINT_BACKEND=linuxkms --setenv="SPACEWARS_CONTROL_SOCKET=$socket" \
  "$root/engine-client" "${args[@]}"
for i in {1..20}; do
  [[ -S "$socket" ]] && break
  sleep 0.5
done
"${cli[@]}" --socket "$socket" ui wait --screen gameplay --scenario pizza --timeout 15s --json > "$out/ui-start.json"
"${cli[@]}" --socket "$socket" host render-trace start-work > "$out/trace-start.txt"
pid=$(systemctl show -p MainPID --value spacewars-pizza-lab.service)
for ((i=0;i<=intervals;i++)); do
  {
    printf 'BEGIN %s\n' "$i"
    cat /proc/uptime
    awk '/^cpu/{print}' /proc/stat
    printf 'PROCESS '; cat "/proc/$pid/stat"
    for task in /proc/"$pid"/task/*/stat; do
      # The export/audio workers may exit between directory enumeration and read.
      if stat=$(cat "$task" 2>/dev/null); then printf 'THREAD %s\n' "$stat"; fi
    done
    printf 'TEMP '; cat /sys/class/thermal/thermal_zone0/temp
    printf 'FREQ '; cat /sys/devices/system/cpu/cpu0/cpufreq/scaling_cur_freq
    printf 'THROTTLED '; cat /sys/devices/platform/soc/soc:firmware/get_throttled
    printf 'END\n'
  } >> "$out/cpu.txt"
  timeout 8 "${cli[@]}" --socket "$socket" status > "$out/status-$i.txt"
  if grep -q '^kms_trace_state=complete' "$out/status-$i.txt"; then
    "${cli[@]}" --socket "$socket" host render-trace read > "$out/trace-$i.txt"
    "${cli[@]}" --socket "$socket" host render-trace start-work > "$out/trace-start-$i.txt"
  fi
  if ((i<intervals)); then sleep 1; fi
done
"${cli[@]}" --socket "$socket" host render-trace read > "$out/trace-final.txt"
# Screenshots happen after all CPU/timing windows.
"${cli[@]}" --socket "$socket" screenshot "$out/final.png" > "$out/screenshot.txt"
"${cli[@]}" --socket "$socket" host pause --timeout 10s --json > "$out/ui-end.json"
echo "$name complete"
