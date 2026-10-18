// SPDX-License-Identifier: MIT
// Link-time syscall fixtures: this executable cannot open a real watchdog.
#define _GNU_SOURCE
#include <assert.h>
#include <errno.h>
#include <fcntl.h>
#include <linux/watchdog.h>
#include <stdarg.h>
#include <stdbool.h>
#include <stdio.h>
#include <string.h>
#include <sys/stat.h>
#include <time.h>
#include <unistd.h>

int watchdog_entry(int argc, char **argv);

enum { PROPERTY = 10, COMMAND_LINE, BOOT_ID, WATCHDOG, RECORD, FD_LIMIT };
static unsigned char property[8];
static size_t property_size;
static char command_line[17000];
static const char boot_id[] = "12345678-1234-1234-1234-123456789abc\n";
static char record[512];
static size_t record_size;
static size_t offsets[FD_LIMIT];
static int watchdog_opens;
static int watchdog_closes;
static int keepalives;
static int timeout_value;
static unsigned long fail_ioctl;
static bool unsupported;
static bool clamped_timeout;
static bool existing_record;
static bool public_directory;
static bool sync_error;
static bool missing_property;
static uid_t caller;
static unsigned int cases;

static void reset_fixture(void) {
    memset(property, 0, sizeof(property));
    property[3] = 1;
    property_size = 4;
    memset(command_line, 0, sizeof(command_line));
    strcpy(command_line, "root=PARTUUID=12345678-03 spacewars.trial=");
    memset(command_line + strlen(command_line), 'a', 64);
    memset(offsets, 0, sizeof(offsets));
    memset(record, 0, sizeof(record));
    record_size = 0;
    watchdog_opens = watchdog_closes = keepalives = timeout_value = 0;
    fail_ioctl = 0;
    unsupported = clamped_timeout = existing_record = public_directory = sync_error =
        missing_property = false;
    caller = 0;
}

uid_t __wrap_geteuid(void) { return caller; }

int __wrap_open(const char *path, int flags, ...) {
    assert((flags & O_CLOEXEC) != 0 && (flags & O_NOFOLLOW) != 0);
    if (strcmp(path, "/sys/firmware/devicetree/base/chosen/bootloader/tryboot") == 0) {
        if (missing_property) {
            errno = ENOENT;
            return -1;
        }
        return PROPERTY;
    }
    if (strcmp(path, "/proc/cmdline") == 0) {
        return COMMAND_LINE;
    }
    if (strcmp(path, "/proc/sys/kernel/random/boot_id") == 0) {
        return BOOT_ID;
    }
    if (strcmp(path, "/dev/watchdog0") == 0) {
        assert((flags & O_ACCMODE) == O_WRONLY);
        ++watchdog_opens;
        return WATCHDOG;
    }
    assert(strcmp(path, "/run/spacewars-recovery/early-watchdog.json") == 0);
    assert((flags & (O_EXCL | O_CREAT)) == (O_EXCL | O_CREAT));
    va_list arguments;
    va_start(arguments, flags);
    assert(va_arg(arguments, int) == 0600);
    va_end(arguments);
    return RECORD;
}

ssize_t __wrap_read(int fd, void *buffer, size_t capacity) {
    assert(fd >= PROPERTY && fd <= BOOT_ID);
    const void *data = property;
    size_t length = property_size;
    if (fd == COMMAND_LINE) {
        data = command_line;
        length = strlen(command_line);
    }
    if (fd == BOOT_ID) {
        data = boot_id;
        length = strlen(boot_id);
    }
    size_t count = length - offsets[fd];
    if (count > capacity) {
        count = capacity;
    }
    if (count > 7) {
        count = 7;
    } // Exercise short reads in production parsing.
    memcpy(buffer, (const char *)data + offsets[fd], count);
    offsets[fd] += count;
    return (ssize_t)count;
}

int __wrap_close(int fd) {
    assert(fd >= PROPERTY && fd <= RECORD);
    if (fd == WATCHDOG) {
        ++watchdog_closes;
    }
    return 0;
}

ssize_t __wrap_write(int fd, const void *buffer, size_t size) {
    assert(fd == RECORD); // No magic-close or other write to the watchdog.
    if (size > 11) {
        size = 11;
    } // Exercise partial writes, too.
    assert(record_size + size < sizeof(record));
    memcpy(record + record_size, buffer, size);
    record_size += size;
    return (ssize_t)size;
}

int __wrap_ioctl(int fd, unsigned long request, ...) {
    assert(fd == WATCHDOG);
    assert(request == WDIOC_GETSUPPORT || request == WDIOC_SETTIMEOUT ||
           request == WDIOC_GETTIMEOUT || request == WDIOC_KEEPALIVE);
    // In particular, the helper must never disable the watchdog on cleanup.
    if (request == fail_ioctl) {
        errno = EIO;
        return -1;
    }
    if (request == WDIOC_KEEPALIVE) {
        ++keepalives;
        return 0;
    }
    va_list arguments;
    va_start(arguments, request);
    void *pointer = va_arg(arguments, void *);
    va_end(arguments);
    if (request == WDIOC_GETSUPPORT) {
        struct watchdog_info *info = pointer;
        strcpy((char *)info->identity,
               unsupported ? "unsupported" : "Broadcom BCM2835 Watchdog timer");
        info->options = WDIOF_SETTIMEOUT | WDIOF_MAGICCLOSE | WDIOF_KEEPALIVEPING;
    } else if (request == WDIOC_SETTIMEOUT) {
        assert(*(int *)pointer == 210);
        timeout_value = clamped_timeout ? 15 : *(int *)pointer;
        *(int *)pointer = timeout_value;
    } else {
        *(int *)pointer = timeout_value;
    }
    return 0;
}

int __wrap_mkdir(const char *path, mode_t mode) {
    assert(strcmp(path, "/run/spacewars-recovery") == 0 && mode == 0700);
    return 0;
}

int __wrap_lstat(const char *path, struct stat *value) {
    memset(value, 0, sizeof(*value));
    if (strcmp(path, "/run/spacewars-recovery/early-watchdog.json") == 0) {
        if (existing_record) {
            value->st_mode = S_IFREG | 0600;
            return 0;
        }
        errno = ENOENT;
        return -1;
    }
    assert(strcmp(path, "/run/spacewars-recovery") == 0);
    value->st_mode = S_IFDIR | (public_directory ? 0755 : 0700);
    return 0;
}

int __wrap_fsync(int fd) {
    assert(fd == RECORD);
    if (sync_error) {
        errno = EIO;
        return -1;
    }
    return 0;
}

int __wrap_clock_gettime(clockid_t clock, struct timespec *value) {
    assert(clock == CLOCK_BOOTTIME);
    value->tv_sec = 4;
    value->tv_nsec = 250000000;
    return 0;
}

static void run_case(const char *name, int expected, int opens) {
    char *arguments[] = {"spacewars-trial-watchdog", "--arm-trial", NULL};
    const int result = watchdog_entry(2, arguments);
    if (result != expected || watchdog_opens != opens || watchdog_closes != opens) {
        fprintf(stderr, "%s: status %d expected %d; opened %d closed %d expected %d\n", name,
                result, expected, watchdog_opens, watchdog_closes, opens);
        assert(false);
    }
    ++cases;
}

int main(void) {
    reset_fixture();
    run_case("valid trial", 0, 1);
    assert(timeout_value == 210 && keepalives == 1);
    assert(strstr(record, "\"armed_boottime_ms\":4250") != NULL);
    assert(strstr(record, "\"timeout_seconds\":210") != NULL);
    assert(strstr(record, "12345678-1234-1234-1234-123456789abc") != NULL);
    assert(strstr(record, "\"trial_id\":\"aaaaaaaa") != NULL);

    reset_fixture();
    property[3] = 0;
    run_case("normal boot", 0, 0);
    reset_fixture();
    missing_property = true;
    run_case("no tryboot capability", 0, 0);
    reset_fixture();
    property_size = 3;
    run_case("short property", 1, 0);
    reset_fixture();
    property_size = 5;
    run_case("long property", 1, 0);
    reset_fixture();
    property[0] = 1;
    run_case("wrong byte order", 1, 0);
    reset_fixture();
    property[3] = 2;
    run_case("invalid tryboot value", 1, 0);
    reset_fixture();
    caller = 1000;
    run_case("unprivileged caller", 1, 0);
    reset_fixture();
    existing_record = true;
    run_case("duplicate arm", 1, 0);
    reset_fixture();
    unsupported = true;
    run_case("wrong driver", 1, 1);
    reset_fixture();
    clamped_timeout = true;
    run_case("driver clamps timeout", 1, 1);
    for (size_t i = 0; i < 4; ++i) {
        const unsigned long commands[] = {WDIOC_GETSUPPORT, WDIOC_SETTIMEOUT, WDIOC_GETTIMEOUT,
                                          WDIOC_KEEPALIVE};
        reset_fixture();
        fail_ioctl = commands[i];
        run_case("failed ioctl", 1, 1);
    }
    reset_fixture();
    strcpy(command_line, "root=/dev/sda3");
    run_case("missing token still arms", 1, 1);
    reset_fixture();
    strcat(command_line, " spacewars.trial=bbbb");
    run_case("duplicate token", 1, 1);
    reset_fixture();
    command_line[strlen(command_line) - 1] = 'G';
    run_case("invalid token hex", 1, 1);
    reset_fixture();
    memset(command_line, 'x', sizeof(command_line) - 1);
    run_case("oversized cmdline", 1, 1);
    reset_fixture();
    public_directory = true;
    run_case("public runtime directory", 1, 1);
    reset_fixture();
    sync_error = true;
    run_case("record flush failure", 1, 1);
    printf("watchdog helper: %u syscall-fixture cases passed; no real device accessed\n", cases);
    return 0;
}
