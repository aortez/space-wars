SUMMARY = "Bounded persistent system journal shared by Space-Wars A/B slots"
LICENSE = "MIT"
LIC_FILES_CHKSUM = "file://${COMMON_LICENSE_DIR}/MIT;md5=0835ade698e0bcf8506ecda2f7b4f302"

inherit systemd

SRC_URI = " \
    file://spacewars-journal-prepare.sh \
    file://spacewars-journal.service \
    file://persistent-journal.conf \
    file://spacewars-journal.conf \
"
RDEPENDS:${PN} = "systemd coreutils util-linux-mount util-linux-umount util-linux-mountpoint"

# The flush service pulls this in before it switches journald away from RAM.
SYSTEMD_SERVICE:${PN} = "spacewars-journal.service"
SYSTEMD_AUTO_ENABLE = "disable"

do_install() {
    install -d ${D}${sbindir} ${D}${systemd_system_unitdir}/systemd-journal-flush.service.d ${D}${sysconfdir}/systemd/journald.conf.d
    install -m 0755 ${WORKDIR}/spacewars-journal-prepare.sh ${D}${sbindir}/spacewars-journal-prepare
    install -m 0644 ${WORKDIR}/spacewars-journal.service ${D}${systemd_system_unitdir}/spacewars-journal.service
    install -m 0644 ${WORKDIR}/persistent-journal.conf ${D}${systemd_system_unitdir}/systemd-journal-flush.service.d/spacewars-persistent-journal.conf
    install -m 0644 ${WORKDIR}/spacewars-journal.conf ${D}${sysconfdir}/systemd/journald.conf.d/spacewars-journal.conf
}

FILES:${PN} += "${systemd_system_unitdir}/systemd-journal-flush.service.d/spacewars-persistent-journal.conf"
