FILESEXTRAPATHS:prepend := "${THISDIR}/files:"

SRC_URI += "file://spacewars-legacy-update-disabled"
do_install:append() {
    install -m 0755 ${WORKDIR}/spacewars-legacy-update-disabled ${D}${sbindir}/ab-update
    install -m 0755 ${WORKDIR}/spacewars-legacy-update-disabled ${D}${sbindir}/ab-update-with-key
}
