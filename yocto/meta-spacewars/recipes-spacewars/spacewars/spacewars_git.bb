SUMMARY = "Space-Wars Rust kiosk binaries"
DESCRIPTION = "Rust + Slint Space-Wars client and support binaries."
HOMEPAGE = "https://github.com/aortez/space-wars"
LICENSE = "MIT"
LIC_FILES_CHKSUM = "file://${COMMON_LICENSE_DIR}/MIT;md5=0835ade698e0bcf8506ecda2f7b4f302"

inherit externalsrc cargo_bin systemd deploy

SPACEWARS_SRCROOT = "${@os.path.realpath('${THISDIR}/../../../..')}"
EXTERNALSRC = "${SPACEWARS_SRCROOT}"
CARGO_MANIFEST_PATH = "${SPACEWARS_SRCROOT}/Cargo.toml"
SPACEWARS_GPU ?= "0"
SPACEWARS_KIOSK_FEATURE = "${@'pi-kiosk-gpu' if d.getVar('SPACEWARS_GPU') == '1' else 'pi-kiosk'}"
EXTRA_CARGO_FLAGS = "--locked --workspace --no-default-features --features engine-client/${SPACEWARS_KIOSK_FEATURE}"
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
DEPENDS:append = "${@' virtual/egl virtual/libgles2 virtual/libgbm' if d.getVar('SPACEWARS_GPU') == '1' else ''}"

RDEPENDS:${PN} += " \
    alsa-lib \
    bash \
    coreutils \
    e2fsprogs \
    e2fsprogs-e2fsck \
    openssh-keygen \
    fontconfig \
    libdrm \
    libinput \
    libxkbcommon \
    seatd \
    libudev \
    sudo \
    systemd \
    util-linux-flock \
    util-linux-setpriv \
    xkeyboard-config \
"
# EGL and the driver are loaded at runtime; automatic ELF dependencies alone
# are insufficient. The explicit software-only image needs none of these.
RDEPENDS:${PN}:append = "${@' libegl-mesa libgles2-mesa libgbm mesa-megadriver' if d.getVar('SPACEWARS_GPU') == '1' else ''}"

INSANE_SKIP:${PN}-dbg += "buildpaths"

do_compile[network] = "1"

# Resolve once at recipe parsing, then use the same identity for Cargo and the
# package. externalsrc disables parse caching, including across Git commits.
export SPACEWARS_BUILD_REVISION
python () {
    import json
    from spacewars.image_identity import source_identity

    identity = source_identity(d.getVar("SPACEWARS_SRCROOT"))
    d.setVar("SPACEWARS_SOURCE_IDENTITY", json.dumps(identity, sort_keys=True))
    d.setVar("SPACEWARS_BUILD_REVISION", identity["revision"])
}
do_compile[file-checksums] += "${SPACEWARS_LAYERDIR}/lib/spacewars/image_identity.py:True"
do_install[file-checksums] += "${SPACEWARS_LAYERDIR}/lib/spacewars/image_identity.py:True"

python verify_spacewars_source_identity() {
    import json
    from spacewars.image_identity import source_identity

    if source_identity(d.getVar("SPACEWARS_SRCROOT")) != json.loads(d.getVar("SPACEWARS_SOURCE_IDENTITY")):
        bb.fatal("Space-Wars source identity changed during the build; rebuild from a stable checkout")
}
do_compile[prefuncs] += "verify_spacewars_source_identity"
do_compile[postfuncs] += "verify_spacewars_source_identity"

python write_spacewars_source_identity() {
    from pathlib import Path

    bb.build.exec_func("verify_spacewars_source_identity", d)
    path = Path(d.getVar("D")) / "usr/share/spacewars/source.json"
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(d.getVar("SPACEWARS_SOURCE_IDENTITY") + "\n")
    path.chmod(0o644)
}
do_install[postfuncs] += "write_spacewars_source_identity"

python () {
    srcroot = d.getVar("SPACEWARS_SRCROOT")
    tracked = [
        f"{srcroot}/Cargo.toml:True",
        f"{srcroot}/Cargo.lock:True",
        f"{srcroot}/crates/engine-client/ui/main.slint:True",
        f"{srcroot}/yocto/meta-spacewars/recipes-spacewars/spacewars/files/spacewars-data-init.sh:True",
        f"{srcroot}/yocto/meta-spacewars/recipes-spacewars/spacewars/files/spacewars-data-init.service:True",
        f"{srcroot}/yocto/meta-spacewars/recipes-spacewars/spacewars/files/spacewars-access.service:True",
        f"{srcroot}/yocto/meta-spacewars/recipes-spacewars/spacewars/files/spacewars-sshd-access.conf:True",
        f"{srcroot}/yocto/meta-spacewars/recipes-spacewars/spacewars/files/sshd-spacewars-access.conf:True",
        f"{srcroot}/yocto/meta-spacewars/recipes-spacewars/spacewars/files/spacewars-system.service:True",
        f"{srcroot}/yocto/meta-spacewars/recipes-spacewars/spacewars/files/spacewars-recovery.service:True",
        f"{srcroot}/yocto/meta-spacewars/recipes-spacewars/spacewars/files/spacewars-install.conf:True",
        f"{srcroot}/yocto/meta-spacewars/recipes-spacewars/spacewars/files/spacewars-rtc-capture.service:True",
        f"{srcroot}/yocto/meta-spacewars/recipes-spacewars/spacewars/files/spacewars-kiosk.service:True",
        f"{srcroot}/yocto/meta-spacewars/recipes-spacewars/spacewars/files/spacewars-seatd.service:True",
        f"{srcroot}/yocto/meta-spacewars/recipes-spacewars/spacewars/files/spacewars-fast-update.sh:True",
        f"{srcroot}/yocto/meta-spacewars/recipes-spacewars/spacewars/files/spacewars-fast-update-wrapper.sh:True",
        f"{srcroot}/yocto/meta-spacewars/recipes-spacewars/spacewars/files/spacewars-logs.sh:True",
        f"{srcroot}/yocto/meta-spacewars/recipes-spacewars/spacewars/files/spacewars-logs.sudoers:True",
        f"{srcroot}/yocto/meta-spacewars/recipes-spacewars/spacewars/files/spacewars-renderer.sh:True",
        f"{srcroot}/yocto/meta-spacewars/recipes-spacewars/spacewars/files/spacewars-renderer-wrapper.sh:True",
        f"{srcroot}/yocto/meta-spacewars/recipes-spacewars/spacewars/files/spacewars-renderer.sudoers:True",
        f"{srcroot}/yocto/meta-spacewars/recipes-spacewars/spacewars/files/spacewars-renderer.conf:True",
    ]

    for top in ("crates", "scenarios", "vendor"):
        root_dir = os.path.join(srcroot, top)
        if not os.path.isdir(root_dir):
            continue
        for root, _, files in os.walk(root_dir):
            for name in sorted(files):
                if name.endswith((".rs", ".toml", ".slint", ".glsl", ".wgsl")):
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
    install -d -m 0700 ${D}${libexecdir}/spacewars
    install -m 0700 ${WORKDIR}/spacewars-fast-update.sh ${D}${libexecdir}/spacewars/fast-update
    install -m 0755 ${WORKDIR}/spacewars-fast-update-wrapper.sh ${D}${sbindir}/spacewars-fast-update
    install -m 0755 ${WORKDIR}/spacewars-logs.sh ${D}${sbindir}/spacewars-logs
    install -d -m 0750 ${D}${sysconfdir}/sudoers.d
    install -m 0440 ${WORKDIR}/spacewars-logs.sudoers ${D}${sysconfdir}/sudoers.d/spacewars-logs

    install -d ${D}${systemd_system_unitdir}
    install -m 0644 ${WORKDIR}/spacewars-data-init.service ${D}${systemd_system_unitdir}/spacewars-data-init.service
    install -m 0644 ${WORKDIR}/spacewars-access.service ${D}${systemd_system_unitdir}/spacewars-access.service
    install -d ${D}${systemd_system_unitdir}/sshd.service.d
    install -m 0644 ${WORKDIR}/sshd-spacewars-access.conf ${D}${systemd_system_unitdir}/sshd.service.d/spacewars-access.conf
    install -d ${D}${sysconfdir}/ssh/sshd_config.d
    install -m 0644 ${WORKDIR}/spacewars-sshd-access.conf ${D}${sysconfdir}/ssh/sshd_config.d/10-spacewars-access.conf
    install -m 0644 ${WORKDIR}/spacewars-system.service ${D}${systemd_system_unitdir}/spacewars-system.service
    install -m 0644 ${WORKDIR}/spacewars-recovery.service ${D}${systemd_system_unitdir}/spacewars-recovery.service
    install -d ${D}${nonarch_libdir}/tmpfiles.d
    install -m 0644 ${WORKDIR}/spacewars-install.conf ${D}${nonarch_libdir}/tmpfiles.d/spacewars-install.conf
    install -m 0644 ${WORKDIR}/spacewars-rtc-capture.service ${D}${systemd_system_unitdir}/spacewars-rtc-capture.service
    install -m 0644 ${WORKDIR}/spacewars-kiosk.service ${D}${systemd_system_unitdir}/spacewars-kiosk.service
    install -m 0644 ${WORKDIR}/spacewars-seatd.service ${D}${systemd_system_unitdir}/spacewars-seatd.service

    if [ "${SPACEWARS_GPU}" = "1" ]; then
        install -m 0700 ${WORKDIR}/spacewars-renderer.sh ${D}${libexecdir}/spacewars/renderer
        install -m 0755 ${WORKDIR}/spacewars-renderer-wrapper.sh ${D}${sbindir}/spacewars-renderer
        install -m 0440 ${WORKDIR}/spacewars-renderer.sudoers ${D}${sysconfdir}/sudoers.d/spacewars-renderer
        install -d ${D}${systemd_system_unitdir}/spacewars-kiosk.service.d
        install -m 0644 ${WORKDIR}/spacewars-renderer.conf ${D}${systemd_system_unitdir}/spacewars-kiosk.service.d/renderer.conf
    fi

    install -d ${D}/var/lib
}

SRC_URI += " \
    file://spacewars-data-init.sh \
    file://spacewars-data-init.service \
    file://spacewars-access.service \
    file://spacewars-sshd-access.conf \
    file://sshd-spacewars-access.conf \
    file://spacewars-system.service \
    file://spacewars-recovery.service \
    file://spacewars-install.conf \
    file://spacewars-rtc-capture.service \
    file://spacewars-kiosk.service \
    file://spacewars-seatd.service \
    file://spacewars-fast-update.sh \
    file://spacewars-fast-update-wrapper.sh \
    file://spacewars-logs.sh \
    file://spacewars-logs.sudoers \
    file://spacewars-renderer.sh \
    file://spacewars-renderer-wrapper.sh \
    file://spacewars-renderer.sudoers \
    file://spacewars-renderer.conf \
"

do_install[file-checksums] += "${SPACEWARS_LAYERDIR}/lib/spacewars/fast_update.py:True"
python write_spacewars_fast_compatibility() {
    from pathlib import Path
    from spacewars.fast_update import compatibility_id

    assets = [Path(d.getVar("WORKDIR")) / name for name in (
        "spacewars-data-init.sh", "spacewars-data-init.service",
        "spacewars-access.service", "spacewars-sshd-access.conf", "sshd-spacewars-access.conf",
        "spacewars-system.service", "spacewars-rtc-capture.service",
        "spacewars-recovery.service", "spacewars-install.conf",
        "spacewars-kiosk.service", "spacewars-seatd.service", "spacewars-fast-update.sh",
        "spacewars-fast-update-wrapper.sh",
        "spacewars-logs.sh", "spacewars-logs.sudoers",
    )]
    # A fast client must not assume a newer maintenance/service contract than
    # the installed root image. This binary is not replaced by the fast path.
    assets.append(Path(d.getVar("D")) / "usr/bin/engine-os-manager")
    if d.getVar("SPACEWARS_GPU") == "1":
        assets.extend(Path(d.getVar("WORKDIR")) / name for name in (
            "spacewars-renderer.sh", "spacewars-renderer-wrapper.sh", "spacewars-renderer.sudoers", "spacewars-renderer.conf",
        ))
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

SYSTEMD_SERVICE:${PN} = "spacewars-seatd.service spacewars-data-init.service spacewars-kiosk.service spacewars-system.service spacewars-rtc-capture.service spacewars-recovery.service spacewars-access.service"
SYSTEMD_AUTO_ENABLE = "enable"

FILES:${PN} = " \
    ${bindir}/engine-client \
    ${bindir}/spacewars-cli \
    ${bindir}/engine-agent \
    ${bindir}/engine-os-manager \
    ${bindir}/falling-benchmark \
    ${bindir}/spacewars-data-init \
    ${libexecdir}/spacewars \
    ${sbindir}/spacewars-fast-update \
    ${sbindir}/spacewars-logs \
    ${sysconfdir}/sudoers.d/spacewars-logs \
    ${sbindir}/spacewars-renderer \
    ${sysconfdir}/sudoers.d/spacewars-renderer \
    ${systemd_system_unitdir}/spacewars-kiosk.service.d/renderer.conf \
    ${datadir}/spacewars/fast-update-compat \
    ${datadir}/spacewars/source.json \
    ${systemd_system_unitdir}/spacewars-data-init.service \
    ${systemd_system_unitdir}/spacewars-access.service \
    ${systemd_system_unitdir}/sshd.service.d/spacewars-access.conf \
    ${sysconfdir}/ssh/sshd_config.d/10-spacewars-access.conf \
    ${systemd_system_unitdir}/spacewars-system.service \
    ${systemd_system_unitdir}/spacewars-recovery.service \
    ${nonarch_libdir}/tmpfiles.d/spacewars-install.conf \
    ${systemd_system_unitdir}/spacewars-rtc-capture.service \
    ${systemd_system_unitdir}/spacewars-kiosk.service \
    ${systemd_system_unitdir}/spacewars-seatd.service \
    /var/lib \
"
