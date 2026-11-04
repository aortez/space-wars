SUMMARY = "Immutable pre-root recovery stage for Space-Wars trials"
LICENSE = "MIT"

inherit core-image

# This is a shared boot asset installed only by supervised bootstrap. Normal
# rootfs OTA images cannot replace it. It does not include the kiosk/updater or
# inspect a candidate before the trial watchdog is armed.
PACKAGE_INSTALL = "initramfs-framework-base initramfs-module-udev initramfs-module-rootfs spacewars-trial-watchdog ${VIRTUAL-RUNTIME_base-utils} base-passwd"
PACKAGE_EXCLUDE = "kernel-image-*"
IMAGE_FEATURES = ""
IMAGE_LINGUAS = ""
IMAGE_FSTYPES = "cpio.gz"
IMAGE_NAME_SUFFIX = ""
IMAGE_ROOTFS_SIZE = "8192"
IMAGE_ROOTFS_EXTRA_SPACE = "0"
IMAGE_ROOTFS_MAXSIZE = "32768"
ROOTFS_BOOTSTRAP_INSTALL = ""
NO_RECOMMENDATIONS = "1"

# The framework's split base package does not depend on the base utilities.
# Validate the actual shell/tools and ELF dependency closure, not just package
# names, so a successful build cannot silently produce an unusable /init.
do_rootfs[depends] += "binutils-native:do_populate_sysroot"
do_rootfs[file-checksums] += "${SPACEWARS_LAYERDIR}/lib/spacewars/recovery_rootfs.py:True"
python verify_spacewars_recovery_rootfs() {
    from spacewars.recovery_rootfs import verify_recovery_rootfs

    try:
        verify_recovery_rootfs(d.getVar("IMAGE_ROOTFS"), d.getVar("BUILD_READELF"))
    except ValueError as error:
        bb.fatal(str(error))
}
ROOTFS_POSTPROCESS_COMMAND:append = " verify_spacewars_recovery_rootfs;"
