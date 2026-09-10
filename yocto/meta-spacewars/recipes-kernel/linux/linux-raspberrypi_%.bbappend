FILESEXTRAPATHS:prepend := "${THISDIR}/files:"
SRC_URI:append:raspberrypi-spacewars = " file://spacewars-pi45.cfg"

# Fail early if a BSP update silently selects a Pi 5-only page size.
do_configure:append:raspberrypi-spacewars() {
    grep -q '^CONFIG_ARM64_4K_PAGES=y$' ${B}/.config || bbfatal "Pi 4/5 image requires a 4 KiB-page kernel"
    grep -q '^CONFIG_MFD_RP1=y$' ${B}/.config || bbfatal "Common kernel is missing Pi 5 RP1 support"
}
