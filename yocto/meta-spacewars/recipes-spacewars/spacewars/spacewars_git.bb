SUMMARY = "Space-Wars Rust kiosk binaries"
DESCRIPTION = "Rust + Slint Space-Wars client and support binaries."
HOMEPAGE = "https://github.com/aortez/space-wars"
LICENSE = "MIT"
LIC_FILES_CHKSUM = "file://${COMMON_LICENSE_DIR}/MIT;md5=0835ade698e0bcf8506ecda2f7b4f302"

inherit externalsrc cargo_bin systemd

SPACEWARS_SRCROOT = "${@os.path.realpath('${THISDIR}/../../../..')}"
EXTERNALSRC = "${SPACEWARS_SRCROOT}"
CARGO_MANIFEST_PATH = "${SPACEWARS_SRCROOT}/Cargo.toml"
EXTRA_CARGO_FLAGS = "--locked --workspace --no-default-features --features engine-client/pi-kiosk"
EXTRA_RUSTFLAGS += "--remap-path-prefix=${WORKDIR}=${TARGET_DBGSRC_DIR}"

DEPENDS += " \
    alsa-lib \
    fontconfig \
    freetype \
    libdrm \
    libinput \
    libxkbcommon \
    seatd \
    udev \
"

RDEPENDS:${PN} += " \
    alsa-lib \
    fontconfig \
    libdrm \
    libinput \
    libxkbcommon \
    seatd \
    libudev \
    xkeyboard-config \
"

INSANE_SKIP:${PN}-dbg += "buildpaths"

do_compile[network] = "1"

python () {
    srcroot = d.getVar("SPACEWARS_SRCROOT")
    tracked = [
        f"{srcroot}/Cargo.toml:True",
        f"{srcroot}/Cargo.lock:True",
        f"{srcroot}/crates/engine-client/ui/main.slint:True",
        f"{srcroot}/yocto/meta-spacewars/recipes-spacewars/spacewars/files/spacewars-data-init.sh:True",
        f"{srcroot}/yocto/meta-spacewars/recipes-spacewars/spacewars/files/spacewars-data-init.service:True",
        f"{srcroot}/yocto/meta-spacewars/recipes-spacewars/spacewars/files/spacewars-kiosk.service:True",
        f"{srcroot}/yocto/meta-spacewars/recipes-spacewars/spacewars/files/spacewars-seatd.service:True",
    ]

    for top in ("crates", "scenarios", "vendor"):
        root_dir = os.path.join(srcroot, top)
        if not os.path.isdir(root_dir):
            continue
        for root, _, files in os.walk(root_dir):
            for name in sorted(files):
                if name.endswith((".rs", ".toml", ".slint")):
                    tracked.append(f"{os.path.join(root, name)}:True")

    d.appendVarFlag("do_compile", "file-checksums", " " + " ".join(tracked))
    d.appendVarFlag("do_install", "file-checksums", " " + " ".join(tracked))
}

do_install() {
    install -d ${D}${bindir}
    install -m 0755 ${CARGO_BINDIR}/engine-client ${D}${bindir}/engine-client
    install -m 0755 ${CARGO_BINDIR}/spacewars-cli ${D}${bindir}/spacewars-cli
    install -m 0755 ${CARGO_BINDIR}/engine-agent ${D}${bindir}/engine-agent
    install -m 0755 ${CARGO_BINDIR}/engine-os-manager ${D}${bindir}/engine-os-manager
    install -m 0755 ${CARGO_BINDIR}/falling-benchmark ${D}${bindir}/falling-benchmark
    install -m 0755 ${WORKDIR}/spacewars-data-init.sh ${D}${bindir}/spacewars-data-init

    install -d ${D}${systemd_system_unitdir}
    install -m 0644 ${WORKDIR}/spacewars-data-init.service ${D}${systemd_system_unitdir}/spacewars-data-init.service
    install -m 0644 ${WORKDIR}/spacewars-kiosk.service ${D}${systemd_system_unitdir}/spacewars-kiosk.service
    install -m 0644 ${WORKDIR}/spacewars-seatd.service ${D}${systemd_system_unitdir}/spacewars-seatd.service

    install -d ${D}/var/lib
}

SRC_URI += " \
    file://spacewars-data-init.sh \
    file://spacewars-data-init.service \
    file://spacewars-kiosk.service \
    file://spacewars-seatd.service \
"

SYSTEMD_SERVICE:${PN} = "spacewars-seatd.service spacewars-data-init.service spacewars-kiosk.service"
SYSTEMD_AUTO_ENABLE = "enable"

FILES:${PN} = " \
    ${bindir}/engine-client \
    ${bindir}/spacewars-cli \
    ${bindir}/engine-agent \
    ${bindir}/engine-os-manager \
    ${bindir}/falling-benchmark \
    ${bindir}/spacewars-data-init \
    ${systemd_system_unitdir}/spacewars-data-init.service \
    ${systemd_system_unitdir}/spacewars-kiosk.service \
    ${systemd_system_unitdir}/spacewars-seatd.service \
    /var/lib \
"
