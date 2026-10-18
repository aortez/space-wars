// SPDX-License-Identifier: MIT
// Arm a bounded watchdog before the trial root is mounted. This runs only from
// the immutable initramfs; normal boots never open the watchdog device.
#define _GNU_SOURCE
#include <errno.h>
#include <fcntl.h>
#include <inttypes.h>
#include <linux/watchdog.h>
#include <stdbool.h>
#include <stdint.h>
#include <stdio.h>
#include <string.h>
#include <sys/ioctl.h>
#include <sys/stat.h>
#include <time.h>
#include <unistd.h>

#define TRIAL_TIMEOUT_SECONDS 210
#define MAX_CMDLINE_BYTES 16384
#define TOKEN_BYTES 64
#define TRYBOOT_PATH "/sys/firmware/devicetree/base/chosen/bootloader/tryboot"
#define RECORD_DIRECTORY "/run/spacewars-recovery"
#define RECORD_PATH RECORD_DIRECTORY "/early-watchdog.json"

static int read_bounded(const char *path, char *buffer, size_t capacity, size_t *length) {
    int fd = open(path, O_RDONLY | O_CLOEXEC | O_NOFOLLOW | O_NONBLOCK);
    if (fd < 0) {
        return -errno;
    }
    size_t used = 0;
    int result = 0;
    while (used + 1 < capacity) {
        const ssize_t count = read(fd, buffer + used, capacity - used - 1);
        if (count < 0) {
            if (errno == EINTR) {
                continue;
            }
            result = -errno;
            break;
        }
        if (count == 0) {
            break;
        }
        used += (size_t)count;
    }
    if (result == 0 && used + 1 == capacity) {
        char extra;
        ssize_t count;
        do {
            count = read(fd, &extra, 1);
        } while (count < 0 && errno == EINTR);
        result = count < 0 ? -errno : (count != 0 ? -EFBIG : 0);
    }
    if (close(fd) < 0 && result == 0) {
        result = -errno;
    }
    buffer[used] = '\0';
    *length = used;
    return result;
}

static bool lower_hex(char value) {
    return (value >= '0' && value <= '9') || (value >= 'a' && value <= 'f');
}

static int trial_token(char token[TOKEN_BYTES + 1]) {
    char command_line[MAX_CMDLINE_BYTES + 1];
    size_t length = 0;
    int result = read_bounded("/proc/cmdline", command_line, sizeof(command_line), &length);
    if (result < 0 || memchr(command_line, '\0', length) != NULL) {
        return result < 0 ? result : -EINVAL;
    }
    const char prefix[] = "spacewars.trial=";
    bool found = false;
    char *context = NULL;
    for (char *part = strtok_r(command_line, " \t\r\n", &context); part != NULL;
         part = strtok_r(NULL, " \t\r\n", &context)) {
        if (strncmp(part, prefix, sizeof(prefix) - 1) != 0) {
            continue;
        }
        const char *value = part + sizeof(prefix) - 1;
        if (found || strlen(value) != TOKEN_BYTES) {
            return -EINVAL;
        }
        for (size_t i = 0; i < TOKEN_BYTES; ++i) {
            if (!lower_hex(value[i])) {
                return -EINVAL;
            }
        }
        memcpy(token, value, TOKEN_BYTES + 1);
        found = true;
    }
    return found ? 0 : -EINVAL;
}

static int record_arm(const char *token, uint64_t armed_ms, int timeout) {
    char boot_id[38];
    size_t length = 0;
    int result = read_bounded("/proc/sys/kernel/random/boot_id", boot_id, sizeof(boot_id), &length);
    if (result < 0) {
        return result;
    }
    if (length == 37 && boot_id[36] == '\n') {
        boot_id[--length] = '\0';
    }
    if (length != 36) {
        return -EINVAL;
    }
    for (size_t i = 0; i < length; ++i) {
        const bool separator = i == 8 || i == 13 || i == 18 || i == 23;
        if (separator ? boot_id[i] != '-' : !lower_hex(boot_id[i])) {
            return -EINVAL;
        }
    }
    if (mkdir(RECORD_DIRECTORY, 0700) < 0 && errno != EEXIST) {
        return -errno;
    }
    struct stat directory;
    if (lstat(RECORD_DIRECTORY, &directory) < 0) {
        return -errno;
    }
    if (!S_ISDIR(directory.st_mode) || directory.st_uid != 0 || (directory.st_mode & 077) != 0) {
        return -EPERM;
    }
    char json[384];
    const int size = snprintf(json, sizeof(json),
                              "{\"schema\":1,\"boot_id\":\"%s\",\"trial_id\":\"%s\","
                              "\"armed_boottime_ms\":%" PRIu64 ",\"timeout_seconds\":%d}\n",
                              boot_id, token, armed_ms, timeout);
    if (size < 0 || (size_t)size >= sizeof(json)) {
        return -EOVERFLOW;
    }
    const int fd = open(RECORD_PATH, O_WRONLY | O_CREAT | O_EXCL | O_CLOEXEC | O_NOFOLLOW, 0600);
    if (fd < 0) {
        return -errno;
    }
    size_t written = 0;
    while (written < (size_t)size) {
        const ssize_t count = write(fd, json + written, (size_t)size - written);
        if (count < 0 && errno == EINTR) {
            continue;
        }
        if (count <= 0) {
            result = count < 0 ? -errno : -EIO;
            break;
        }
        written += (size_t)count;
    }
    if (result == 0 && fsync(fd) < 0) {
        result = -errno;
    }
    if (close(fd) < 0 && result == 0) {
        result = -errno;
    }
    return result;
}

static int arm_trial(void) {
    char property[5];
    size_t length = 0;
    int result = read_bounded(TRYBOOT_PATH, property, sizeof(property), &length);
    if (result == -ENOENT) {
        return 0; // Unsupported boot firmware cannot qualify for OTA trials.
    }
    if (result < 0) {
        return result;
    }
    if (length != 4 || property[0] != 0 || property[1] != 0 || property[2] != 0 ||
        (property[3] != 0 && property[3] != 1)) {
        return -EINVAL;
    }
    if (property[3] == 0) {
        return 0;
    }
    struct stat previous;
    if (lstat(RECORD_PATH, &previous) == 0) {
        return -EEXIST; // Do not restart the allowance in this boot.
    }
    if (errno != ENOENT) {
        return -errno;
    }
    const int fd = open("/dev/watchdog0", O_WRONLY | O_CLOEXEC | O_NOFOLLOW);
    if (fd < 0) {
        return -errno;
    }
    struct watchdog_info info = {0};
    int timeout = TRIAL_TIMEOUT_SECONDS;
    struct timespec armed = {0};
    char token[TOKEN_BYTES + 1] = {0};
    const unsigned int required = WDIOF_SETTIMEOUT | WDIOF_MAGICCLOSE | WDIOF_KEEPALIVEPING;
    if (ioctl(fd, WDIOC_GETSUPPORT, &info) < 0) {
        result = -errno;
    } else if (memcmp(info.identity, "Broadcom BCM2835 Watchdog timer", 31) != 0 ||
               info.identity[31] != 0 || (info.options & required) != required) {
        result = -ENOTSUP;
    } else if (ioctl(fd, WDIOC_SETTIMEOUT, &timeout) < 0 ||
               ioctl(fd, WDIOC_GETTIMEOUT, &timeout) < 0) {
        result = -errno;
    } else if (timeout != TRIAL_TIMEOUT_SECONDS) {
        result = -ENOTSUP;
    } else if (ioctl(fd, WDIOC_KEEPALIVE, 0) < 0 || clock_gettime(CLOCK_BOOTTIME, &armed) < 0) {
        result = -errno;
    } else if (armed.tv_sec < 0 || (uint64_t)armed.tv_sec > UINT64_MAX / 1000 - 1 ||
               armed.tv_nsec < 0 || armed.tv_nsec >= 1000000000L) {
        result = -EOVERFLOW;
    } else if ((result = trial_token(token)) == 0) {
        const uint64_t armed_ms = (uint64_t)armed.tv_sec * 1000 + (uint64_t)armed.tv_nsec / 1000000;
        result = record_arm(token, armed_ms, timeout);
    }
    // Deliberately no magic 'V' write or disable ioctl, including failures.
    // The pinned BCM2835 driver/kernel provide a bounded virtual timeout after
    // close. The coordinator must take ownership, or reset must follow.
    if (close(fd) < 0 && result == 0) {
        result = -errno;
    }
    return result;
}

int main(int argc, char **argv) {
    if (argc != 2 || strcmp(argv[1], "--arm-trial") != 0 || geteuid() != 0) {
        fprintf(stderr,
                "spacewars-trial-watchdog: root initramfs --arm-trial operation required\n");
        return 1;
    }
    const int result = arm_trial();
    if (result < 0) {
        fprintf(stderr, "spacewars-trial-watchdog: early recovery setup failed: %s\n",
                strerror(-result));
        return 1;
    }
    return 0;
}
