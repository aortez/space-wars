SUMMARY = "Bounded early watchdog for Space-Wars rootfs trials"
LICENSE = "MIT"
LIC_FILES_CHKSUM = "file://${COMMON_LICENSE_DIR}/MIT;md5=0835ade698e0bcf8506ecda2f7b4f302"

SRC_URI = "file://spacewars-trial-watchdog.c file://00-spacewarswatchdog file://98-spacewarsrun"
S = "${WORKDIR}"

do_compile() {
    ${CC} ${CFLAGS} -std=c11 -Wall -Wextra -Werror -Wconversion -Wshadow \
        ${S}/spacewars-trial-watchdog.c ${LDFLAGS} -o spacewars-trial-watchdog
}

do_install() {
    install -d ${D}${sbindir} ${D}/init.d
    install -m 0755 spacewars-trial-watchdog ${D}${sbindir}/spacewars-trial-watchdog
    install -m 0755 ${S}/00-spacewarswatchdog ${D}/init.d/00-spacewarswatchdog
    install -m 0755 ${S}/98-spacewarsrun ${D}/init.d/98-spacewarsrun
}

RDEPENDS:${PN} = "initramfs-framework-base"
FILES:${PN} = "${sbindir}/spacewars-trial-watchdog /init.d/00-spacewarswatchdog /init.d/98-spacewarsrun"
