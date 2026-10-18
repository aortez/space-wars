# Space-Wars imports seed profiles only on first creation. Never reimport a
# revoked connection from the root image during OTA or rollback.
FILESEXTRAPATHS:prepend := "${THISDIR}/files:"
SRC_URI += "file://spacewars-persistent-network.conf"
RDEPENDS:${PN}:append = " spacewars"

do_install:append() {
    install -d ${D}${systemd_system_unitdir}/NetworkManager.service.d
    install -m 0644 ${WORKDIR}/spacewars-persistent-network.conf ${D}${systemd_system_unitdir}/NetworkManager.service.d/spacewars-persistent-network.conf
}

FILES:${PN} += "${systemd_system_unitdir}/NetworkManager.service.d/spacewars-persistent-network.conf"
