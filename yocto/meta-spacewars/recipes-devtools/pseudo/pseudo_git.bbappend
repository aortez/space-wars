# GNU tar's jailified extraction path uses openat2. The pseudo revision in
# Scarthgap only provides an ENOSYS stub, which makes packaging fail on newer
# hosts with "Cannot mkdir: Function not implemented".
#
# Pin the upstream openat2 implementation based on Scarthgap's existing pseudo
# revision so builds remain reproducible on those hosts.
SRC_URI:remove = "git://git.yoctoproject.org/pseudo;branch=master;protocol=https"
SRC_URI:prepend = "git://git.yoctoproject.org/pseudo;protocol=https;nobranch=1 "
SRCREV = "54f3d1b4dd3eaed2c57b43c3a4d62cdf99239ed2"
