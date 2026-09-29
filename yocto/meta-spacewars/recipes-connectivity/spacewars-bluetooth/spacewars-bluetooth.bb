SUMMARY = "Persistent Bluetooth controller pairings for Space-Wars"
LICENSE = "MIT"
LIC_FILES_CHKSUM = "file://${COMMON_LICENSE_DIR}/MIT;md5=0835ade698e0bcf8506ecda2f7b4f302"

inherit systemd

SRC_URI = " \
    file://spacewars-bluetooth-data-init.sh \
    file://spacewars-bluetooth-data.service \
    file://persistent-data.conf \
"
RDEPENDS:${PN} = "bluez5 coreutils util-linux-mount util-linux-umount util-linux-mountpoint"

# Pulled in by bluetooth.service, including restarts, not a separate boot race.
SYSTEMD_SERVICE:${PN} = "spacewars-bluetooth-data.service"
SYSTEMD_AUTO_ENABLE = "disable"

do_install() {
    install -d ${D}${sbindir} ${D}${systemd_system_unitdir}/bluetooth.service.d
    install -m 0755 ${WORKDIR}/spacewars-bluetooth-data-init.sh ${D}${sbindir}/spacewars-bluetooth-data-init
    install -m 0644 ${WORKDIR}/spacewars-bluetooth-data.service ${D}${systemd_system_unitdir}/spacewars-bluetooth-data.service
    install -m 0644 ${WORKDIR}/persistent-data.conf ${D}${systemd_system_unitdir}/bluetooth.service.d/spacewars-persistent-data.conf
}

FILES:${PN} += "${systemd_system_unitdir}/bluetooth.service.d/spacewars-persistent-data.conf"
