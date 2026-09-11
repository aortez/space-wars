SUMMARY = "Per-device Space-Wars display, audio and HAT profiles"
LICENSE = "MIT"
LIC_FILES_CHKSUM = "file://${COMMON_LICENSE_DIR}/MIT;md5=0835ade698e0bcf8506ecda2f7b4f302"

inherit systemd deploy
SRC_URI = " \
    file://spacewars-device.txt \
    file://hdmi.txt \
    file://hyperpixel.txt \
    file://picade.txt \
    file://70-spacewars-picade.rules \
    file://spacewars-hardware.sh \
    file://spacewars-hardware.service \
"
RDEPENDS:${PN} = "coreutils grep sed"
SYSTEMD_SERVICE:${PN} = "spacewars-hardware.service"
SYSTEMD_AUTO_ENABLE = "enable"

do_install() {
    install -d ${D}${bindir} ${D}${systemd_system_unitdir} ${D}${nonarch_base_libdir}/udev/rules.d
    install -m 0755 ${WORKDIR}/spacewars-hardware.sh ${D}${bindir}/spacewars-hardware
    install -m 0644 ${WORKDIR}/spacewars-hardware.service ${D}${systemd_system_unitdir}/
    install -m 0644 ${WORKDIR}/70-spacewars-picade.rules ${D}${nonarch_base_libdir}/udev/rules.d/
}

do_deploy() {
    install -d ${DEPLOYDIR}/spacewars-profiles
    install -m 0644 ${WORKDIR}/spacewars-device.txt ${DEPLOYDIR}/
    for profile in hdmi hyperpixel picade; do
        install -m 0644 ${WORKDIR}/$profile.txt ${DEPLOYDIR}/spacewars-profiles/
    done
}
addtask deploy after do_install before do_build

FILES:${PN} += "${nonarch_base_libdir}/udev/rules.d"
