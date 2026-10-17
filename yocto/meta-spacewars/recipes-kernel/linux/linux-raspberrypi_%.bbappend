FILESEXTRAPATHS:prepend := "${THISDIR}/files:"
SRC_URI:append:raspberrypi-spacewars = " \
    file://spacewars-pi45.cfg \
    file://0001-watchdog-bcm2835-clamp-hardware-timeout.patch \
    file://test-bcm2835-watchdog.py \
"

# Exercise the actual patched driver, with MMIO intercepted on the build host.
# Checking WDIOC_GETTIMEOUT alone cannot detect a wrapped hardware counter.
do_compile:prepend:raspberrypi-spacewars() {
    python3 ${WORKDIR}/test-bcm2835-watchdog.py \
        --source ${S}/drivers/watchdog/bcm2835_wdt.c \
        --cc "${BUILD_CC} ${BUILD_CFLAGS} ${BUILD_LDFLAGS}"
}

# Fail early if a BSP update silently selects a Pi 5-only page size.
do_configure:append:raspberrypi-spacewars() {
    grep -q '^CONFIG_ARM64_4K_PAGES=y$' ${B}/.config || bbfatal "Pi 4/5 image requires a 4 KiB-page kernel"
    grep -q '^CONFIG_MFD_RP1=y$' ${B}/.config || bbfatal "Common kernel is missing Pi 5 RP1 support"
}

# /sys/kernel/notes reports the loaded kernel's GNU build ID. Export it from
# the same vmlinux that produced Image; a file hash alone describes disk bytes.
python write_spacewars_kernel_build_id() {
    import re
    import subprocess
    from pathlib import Path

    if d.getVar("MACHINE") != "raspberrypi-spacewars":
        return
    output = subprocess.check_output([d.getVar("READELF"), "-n",
                                      str(Path(d.getVar("B")) / "vmlinux")], text=True)
    identities = re.findall(r"Build ID: ([0-9a-f]+)", output)
    if len(identities) != 1 or not re.fullmatch(r"[0-9a-f]{40}", identities[0]):
        bb.fatal("Common Pi kernel must expose one SHA-1 GNU build ID")
    (Path(d.getVar("DEPLOYDIR")) / "spacewars-kernel-build-id").write_text(identities[0] + "\n")
}
do_deploy[postfuncs] += "write_spacewars_kernel_build_id"
