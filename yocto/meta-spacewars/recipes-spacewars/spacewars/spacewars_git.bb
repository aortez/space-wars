SUMMARY = "Space-Wars Rust kiosk binaries"
DESCRIPTION = "Rust + Slint Space-Wars client and support binaries."
HOMEPAGE = "https://github.com/aortez/space-wars"
LICENSE = "MIT"
LIC_FILES_CHKSUM = "file://${COMMON_LICENSE_DIR}/MIT;md5=0835ade698e0bcf8506ecda2f7b4f302"

inherit externalsrc cargo_bin systemd deploy

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
    util-linux \
"

RDEPENDS:${PN} += " \
    alsa-lib \
    bash \
    coreutils \
    fontconfig \
    libdrm \
    libinput \
    libxkbcommon \
    seatd \
    libudev \
    util-linux-flock \
    util-linux-setpriv \
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
        f"{srcroot}/yocto/meta-spacewars/recipes-spacewars/spacewars/files/spacewars-fast-update.sh:True",
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

    install -d ${D}${sbindir}
    install -m 0755 ${WORKDIR}/spacewars-fast-update.sh ${D}${sbindir}/spacewars-fast-update

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
    file://spacewars-fast-update.sh \
"

do_install[file-checksums] += "${SPACEWARS_LAYERDIR}/lib/spacewars/fast_update.py:True"
python write_spacewars_fast_compatibility() {
    from pathlib import Path
    from spacewars.fast_update import compatibility_id

    assets = [Path(d.getVar("WORKDIR")) / name for name in (
        "spacewars-data-init.sh", "spacewars-data-init.service",
        "spacewars-kiosk.service", "spacewars-seatd.service", "spacewars-fast-update.sh",
    )]
    variables = {key: d.getVar(key) or "" for key in (
        "MACHINE", "TARGET_SYS", "TUNE_FEATURES", "DISTRO", "DISTRO_VERSION",
        "DISTRO_FEATURES", "TCLIBC",
    )}
    identity = compatibility_id(d.getVar("RECIPE_SYSROOT"), assets, variables)
    dest = Path(d.getVar("D")) / "usr/share/spacewars"
    dest.mkdir(parents=True, exist_ok=True)
    (dest / "fast-update-compat").write_text(identity + "\n")
}
do_install[postfuncs] += "write_spacewars_fast_compatibility"

# Export the stripped package binaries, not a guessed Cargo build directory or
# an old rootfs image. Building --target spacewars produces this small bundle.
python do_deploy() {
    import hashlib
    import json
    import shutil
    from pathlib import Path

    package = Path(d.getVar("PKGDEST")) / d.getVar("PN")
    dest = Path(d.getVar("DEPLOYDIR")) / "spacewars-fast"
    dest.mkdir(parents=True, exist_ok=True)
    manifest = {
        "format": 1,
        "machine": d.getVar("MACHINE"),
        "target_arch": d.getVar("TARGET_ARCH"),
        "compatibility": (package / "usr/share/spacewars/fast-update-compat").read_text().strip(),
        "binaries": {},
    }
    for name in ("engine-client", "spacewars-cli"):
        source = package / "usr/bin" / name
        content = source.read_bytes()
        manifest["binaries"][name] = {
            "sha256": hashlib.sha256(content).hexdigest(), "size": len(content),
        }
        shutil.copy2(source, dest / name)
    (dest / "manifest.json").write_text(json.dumps(manifest, indent=2) + "\n")
}
addtask deploy after do_package before do_build

SYSTEMD_SERVICE:${PN} = "spacewars-seatd.service spacewars-data-init.service spacewars-kiosk.service"
SYSTEMD_AUTO_ENABLE = "enable"

FILES:${PN} = " \
    ${bindir}/engine-client \
    ${bindir}/spacewars-cli \
    ${bindir}/engine-agent \
    ${bindir}/engine-os-manager \
    ${bindir}/falling-benchmark \
    ${bindir}/spacewars-data-init \
    ${sbindir}/spacewars-fast-update \
    ${datadir}/spacewars/fast-update-compat \
    ${systemd_system_unitdir}/spacewars-data-init.service \
    ${systemd_system_unitdir}/spacewars-kiosk.service \
    ${systemd_system_unitdir}/spacewars-seatd.service \
    /var/lib \
"
