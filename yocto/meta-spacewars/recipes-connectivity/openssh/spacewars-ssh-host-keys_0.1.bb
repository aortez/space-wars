SUMMARY = "Persistent SSH host keys for Space-Wars"
LICENSE = "MIT"
LIC_FILES_CHKSUM = "file://${COMMON_LICENSE_DIR}/MIT;md5=0835ade698e0bcf8506ecda2f7b4f302"

SRC_URI = " \
    file://spacewars-ssh-host-keys \
    file://spacewars-ssh-host-keys.service \
    file://spacewars-sshd-hostkeys.conf \
    file://sshdgenkeys-spacewars.conf \
"

S = "${WORKDIR}"

inherit systemd

SYSTEMD_SERVICE:${PN} = "spacewars-ssh-host-keys.service"
SYSTEMD_AUTO_ENABLE = "enable"

do_install() {
    install -d ${D}${sbindir}
    install -m 0755 ${WORKDIR}/spacewars-ssh-host-keys ${D}${sbindir}/spacewars-ssh-host-keys

    install -d ${D}${systemd_system_unitdir}
    install -m 0644 ${WORKDIR}/spacewars-ssh-host-keys.service ${D}${systemd_system_unitdir}/spacewars-ssh-host-keys.service

    install -d ${D}${systemd_system_unitdir}/sshdgenkeys.service.d
    install -m 0644 ${WORKDIR}/sshdgenkeys-spacewars.conf ${D}${systemd_system_unitdir}/sshdgenkeys.service.d/spacewars-host-keys.conf

    install -d ${D}${sysconfdir}/ssh/sshd_config.d
    install -m 0644 ${WORKDIR}/spacewars-sshd-hostkeys.conf ${D}${sysconfdir}/ssh/sshd_config.d/20-spacewars-host-keys.conf
}

FILES:${PN} = " \
    ${sbindir}/spacewars-ssh-host-keys \
    ${systemd_system_unitdir}/spacewars-ssh-host-keys.service \
    ${systemd_system_unitdir}/sshdgenkeys.service.d/spacewars-host-keys.conf \
    ${sysconfdir}/ssh/sshd_config.d/20-spacewars-host-keys.conf \
"

RDEPENDS:${PN} = " \
    ab-boot-manager \
    openssh-keygen \
"
