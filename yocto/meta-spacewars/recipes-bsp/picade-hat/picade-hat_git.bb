SUMMARY = "Pimoroni Picade X HAT device-tree overlay"
HOMEPAGE = "https://github.com/pimoroni/picade-hat"
# Upstream does not declare a license. Do not mislabel its source as this
# project's MIT license. Review redistribution terms before publishing images.
LICENSE = "CLOSED"

SRC_URI = " \
    git://github.com/pimoroni/picade-hat.git;protocol=https;branch=master \
    file://0001-expose-joystick-as-digital-hat-axes.patch \
    file://0002-separate-utility-keys-from-gamepad.patch \
"
SRCREV = "f97c2ac7211c8d9ff866b22a82c3b2ae6d01e0df"
S = "${WORKDIR}/git"

inherit deploy
DEPENDS = "dtc-native"
COMPATIBLE_MACHINE = "raspberrypi-spacewars"
PACKAGE_ARCH = "${MACHINE_ARCH}"

do_compile() {
    dtc -@ -H epapr -I dts -O dtb -o picade.dtbo ${S}/picade.dts
}

# Validate the real, compiled upstream overlay as well as the host-test fixture.
python validate_picade_inputs() {
    from pathlib import Path
    from spacewars.picade_overlay import validate_picade_overlay

    try:
        validate_picade_overlay(
            Path(d.getVar("B")) / "picade.dtbo",
            Path(d.getVar("STAGING_BINDIR_NATIVE")) / "fdtget",
        )
    except ValueError as error:
        bb.fatal(str(error))
}
do_compile[postfuncs] += "validate_picade_inputs"
do_compile[file-checksums] += "${SPACEWARS_LAYERDIR}/lib/spacewars/picade_overlay.py:True"

do_deploy() {
    install -d ${DEPLOYDIR}
    install -m 0644 picade.dtbo ${DEPLOYDIR}/picade.dtbo
}
addtask deploy after do_compile before do_build
