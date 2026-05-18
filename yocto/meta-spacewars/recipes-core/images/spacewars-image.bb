SUMMARY = "Space-Wars kiosk image for Raspberry Pi"
DESCRIPTION = "A minimal Raspberry Pi image that boots directly into the Space-Wars Slint client."
LICENSE = "MIT"

inherit pi-base-image
inherit extrausers

# The upstream pi-base image class asks for networkmanager-nmtui, but the
# NetworkManager recipe in this layer set does not emit that split package.
IMAGE_INSTALL:remove = "networkmanager-nmtui"

# USB boot. Keep this aligned with kas-spacewars.yml.
BOOT_DEVICE = "sda"

HOSTNAME_DEFAULT = "spacewars"

# Runtime user for the kiosk process.
EXTRA_USERS_PARAMS = " \
    groupadd -g 1000 spacewars; \
    useradd -m -u 1000 -s /bin/bash -g spacewars -G input,video,audio spacewars; \
    usermod -p '*' spacewars; \
"

setup_spacewars_home() {
    install -d -m 700 ${IMAGE_ROOTFS}/home/spacewars/.ssh
    touch ${IMAGE_ROOTFS}/home/spacewars/.ssh/authorized_keys
    chmod 600 ${IMAGE_ROOTFS}/home/spacewars/.ssh/authorized_keys
    chown -R spacewars:spacewars ${IMAGE_ROOTFS}/home/spacewars
}
ROOTFS_POSTPROCESS_COMMAND:append = " setup_spacewars_home;"

setup_spacewars_ota_sudoers() {
    install -d -m 0750 ${IMAGE_ROOTFS}/etc/sudoers.d
    cat > ${IMAGE_ROOTFS}/etc/sudoers.d/spacewars-ota << 'EOF'
# Allow the kiosk user to perform A/B OTA updates through yocto/scripts/yolo-update.mjs.
spacewars ALL=(root) NOPASSWD: /usr/sbin/ab-update-with-key *, /usr/bin/systemctl reboot
EOF
    chmod 0440 ${IMAGE_ROOTFS}/etc/sudoers.d/spacewars-ota
}
ROOTFS_POSTPROCESS_COMMAND:append = " setup_spacewars_ota_sudoers;"

disable_text_console_for_kiosk() {
    install -d ${IMAGE_ROOTFS}/etc/systemd/system
    ln -sf /dev/null ${IMAGE_ROOTFS}/etc/systemd/system/getty@tty1.service
}
ROOTFS_POSTPROCESS_COMMAND:append = " disable_text_console_for_kiosk;"

setup_hyperpixel_backlight() {
    install -d ${IMAGE_ROOTFS}/etc/systemd/system

    cat > ${IMAGE_ROOTFS}/etc/systemd/system/hyperpixel-backlight.service << 'EOF'
[Unit]
Description=Enable HyperPixel backlight
After=systemd-udev-settle.service
Wants=systemd-udev-settle.service

[Service]
Type=oneshot
ExecStart=/bin/sh -c 'for i in $(seq 1 50); do if [ -e /sys/class/backlight/backlight/bl_power ]; then echo 0 > /sys/class/backlight/backlight/bl_power; echo 1 > /sys/class/backlight/backlight/brightness; exit 0; fi; sleep 0.1; done; exit 0'

[Install]
WantedBy=sysinit.target
EOF

    install -d ${IMAGE_ROOTFS}/etc/systemd/system/sysinit.target.wants
    ln -sf ../hyperpixel-backlight.service ${IMAGE_ROOTFS}/etc/systemd/system/sysinit.target.wants/hyperpixel-backlight.service
}
ROOTFS_POSTPROCESS_COMMAND:append = " setup_hyperpixel_backlight;"

setup_spacewars_coredumps() {
    install -d ${IMAGE_ROOTFS}/etc/systemd
    cat > ${IMAGE_ROOTFS}/etc/systemd/coredump.conf << 'EOF'
[Coredump]
Storage=external
Compress=yes
MaxUse=128M
KeepFree=256M
EOF

    rm -rf ${IMAGE_ROOTFS}/var/lib/systemd/coredump
    ln -s /data/spacewars/coredumps ${IMAGE_ROOTFS}/var/lib/systemd/coredump
}
ROOTFS_POSTPROCESS_COMMAND:append = " setup_spacewars_coredumps;"

IMAGE_INSTALL:append = " \
    alsa-utils \
    file \
    jq \
    nmon \
    rsync \
    screen \
    strace \
    tree \
    vim \
"

IMAGE_INSTALL:append = " \
    linux-firmware-rpidistro-bcm43455 \
    linux-firmware-rpidistro-bcm43456 \
"

IMAGE_INSTALL:append = " \
    spacewars \
    spacewars-ssh-host-keys \
"
