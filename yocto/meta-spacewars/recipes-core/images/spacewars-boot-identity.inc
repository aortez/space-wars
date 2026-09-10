# Rootfs-only OTA must not pair a new kernel's modules with an old boot kernel.
# A different boot ID requires a full USB image flash, not an A/B rootfs update.
do_rootfs[depends] += "virtual/kernel:do_deploy rpi-config:do_deploy rpi-bootfiles:do_deploy picade-hat:do_deploy spacewars-hardware:do_deploy"
do_rootfs[file-checksums] += "${SPACEWARS_LAYERDIR}/lib/spacewars/boot_identity.py:True"

python write_spacewars_boot_identity() {
    from pathlib import Path
    from spacewars.boot_identity import boot_identity

    deploy = Path(d.getVar("DEPLOY_DIR_IMAGE"))
    try:
        identity = boot_identity(deploy, d.getVar("IMAGE_BOOT_FILES")) + "\n"
    except ValueError as error:
        bb.fatal(str(error))
    (deploy / "spacewars-boot-id").write_text(identity)
    config = Path(d.getVar("IMAGE_ROOTFS")) / "etc/spacewars"
    config.mkdir(parents=True, exist_ok=True)
    (config / "boot-id").write_text(identity)
}
ROOTFS_POSTPROCESS_COMMAND:append = " write_spacewars_boot_identity;"

python do_image_complete:append() {
    from pathlib import Path

    identity = (Path(d.getVar("IMAGE_ROOTFS")) / "etc/spacewars/boot-id").read_text()
    deploy = Path(d.getVar("IMGDEPLOYDIR"))
    for name in (d.getVar("IMAGE_NAME"), d.getVar("IMAGE_LINK_NAME")):
        for suffix in (".ext4.gz.boot-id", ".wic.gz.boot-id"):
            (deploy / (name + suffix)).write_text(identity)
}
