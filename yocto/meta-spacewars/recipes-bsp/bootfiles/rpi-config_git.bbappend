# The shared image explicitly selects a board-specific KMS overlay in its
# conditional config. Remove the BSP's unconditional default to avoid applying
# two overlays or depending on the firmware's overlay-map redirection.
do_deploy:append:raspberrypi-spacewars() {
    sed -i '/^dtoverlay=vc4-kms-v3d$/d' ${DEPLOYDIR}/${BOOTFILES_DIR_NAME}/config.txt
}
