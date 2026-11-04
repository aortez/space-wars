# Hash final stripped binaries and rootfs contents after image preprocessing.
# A publisher must reject dirty source and bind this record with the signed
# expanded-payload hash. Protocol numbers do not establish board qualification.
do_image[file-checksums] += "${SPACEWARS_LAYERDIR}/lib/spacewars/image_identity.py:True"
python do_image:append() {
    from spacewars.image_identity import write_image_identity

    try:
        write_image_identity(d.getVar("IMAGE_ROOTFS"), d.getVar("MACHINE"),
                             d.getVar("TARGET_ARCH"), int(d.getVar("SOURCE_DATE_EPOCH")))
    except (OSError, ValueError) as error:
        bb.fatal(str(error))
}

python do_image_complete:append() {
    from pathlib import Path

    identity = (Path(d.getVar("IMAGE_ROOTFS")) / "etc/spacewars/image.json").read_bytes()
    deploy = Path(d.getVar("IMGDEPLOYDIR"))
    for name in (d.getVar("IMAGE_NAME"), d.getVar("IMAGE_LINK_NAME")):
        for suffix in (".ext4.gz.image.json", ".wic.gz.image.json"):
            (deploy / (name + suffix)).write_bytes(identity)
}
