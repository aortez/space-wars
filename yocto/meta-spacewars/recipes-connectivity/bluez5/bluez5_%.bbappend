FILESEXTRAPATHS:prepend := "${THISDIR}/files:"

# BlueZ 5.72 sizes the extended advertising data command 8 bytes too large,
# and the 6.12 kernel rejects it, so no BLE advertisement can be registered.
# Backported from BlueZ 2a6968b40378; drop once the recipe moves past 5.86.
SRC_URI += "file://0001-advertising-fix-sending-extra-bytes-with-MGMT_OP_ADD_EXT_ADV_DATA.patch"
