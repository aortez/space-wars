FILESEXTRAPATHS:prepend := "${THISDIR}/files:"

# This exact provider version is qualified with the native D-Bus controls.
SRC_URI:append = " file://0001-settings-retain-version-on-identical-secret-refresh.patch"
