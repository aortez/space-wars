FILESEXTRAPATHS:prepend := "${THISDIR}/files:"

# Fail closed on other provider versions until the receipt contract is reviewed.
SRC_URI:append = " file://0001-timesync-successful-correction-receipt.patch"
