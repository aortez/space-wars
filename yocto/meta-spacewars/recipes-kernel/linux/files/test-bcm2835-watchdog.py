#!/usr/bin/env python3
"""Compile the real BCM2835 start function with intercepted register accesses."""
import argparse
from pathlib import Path
import re
import shlex
import subprocess
import tempfile


def harness(source):
    # Keep the driver code and its advertised heartbeat together. Fail rather
    # than silently testing a copied implementation after an upstream change.
    start = source.index("static int bcm2835_wdt_start(")
    opening = source.index("{", start)
    depth = 1
    ending = opening + 1
    while depth:
        if source[ending] == "{":
            depth += 1
        elif source[ending] == "}":
            depth -= 1
        ending += 1
    function = source[start:ending]
    definitions = "\n".join(re.findall(r"^#define (?:PM_|SECS_|WDOG_).*", source, re.M))
    heartbeat = re.search(r"\.max_hw_heartbeat_ms\s*=\s*([^,]+),", source).group(1)
    return r'''
#include <assert.h>
#include <limits.h>
#include <stdint.h>
#include <stdio.h>

typedef uint32_t u32;
#define min_t(type, a, b) ((type)(a) < (type)(b) ? (type)(a) : (type)(b))
#define spin_lock_irqsave(lock, flags) ((void)(lock), (flags) = 0)
#define spin_unlock_irqrestore(lock, flags) ((void)(lock), (void)(flags))
struct watchdog_device { unsigned int timeout; };
struct bcm2835_wdt { unsigned char *base; int lock; };
static uint32_t registers[16];
static struct bcm2835_wdt device = { .base = (unsigned char *)registers };
static struct bcm2835_wdt *watchdog_get_drvdata(struct watchdog_device *wdog) {
    (void)wdog;
    return &device;
}
static void writel_relaxed(uint32_t value, void *address) {
    *(uint32_t *)address = value;
}
static uint32_t readl_relaxed(const void *address) {
    return *(const uint32_t *)address;
}
''' + "\n" + definitions + "\n" + function + r'''

int main(void) {
    const unsigned int timeouts[] = {
        1, 2, 8, 14, 15, 16, 17, 30, 31, 32, 60, 180, 210, 255, 256,
        65535, 65536, 0x80000000U, UINT_MAX
    };
    const unsigned int heartbeat_ms = HEARTBEAT;
    for (unsigned int i = 0; i < sizeof(timeouts) / sizeof(timeouts[0]); ++i) {
        const unsigned int timeout = timeouts[i];
        struct watchdog_device wdog = { .timeout = timeout };
        registers[PM_RSTC / 4] = 0x80000010;
        assert(bcm2835_wdt_start(&wdog) == 0);
        const uint32_t programmed = registers[PM_WDOG / 4];
        const unsigned int ticks = programmed & PM_WDOG_TIME_SET;
        const unsigned int seconds = timeout < 15 ? timeout : 15;
        assert((programmed & ~(uint32_t)PM_WDOG_TIME_SET) == PM_PASSWORD);
        assert(ticks == seconds * 65536U);
        assert(wdog.timeout == timeout);
        assert(registers[PM_RSTC / 4] ==
               (PM_PASSWORD | 0x80000000U | PM_RSTC_WRCFG_FULL_RESET));
        // The core schedules its last ping one advertised hardware heartbeat
        // before the virtual deadline. The physical counter must match it.
        if (timeout > 15) {
            assert((uint64_t)ticks * 1000 == (uint64_t)heartbeat_ms * 65536);
            const uint64_t last_ping_ms = (uint64_t)timeout * 1000 - heartbeat_ms;
            assert(last_ping_ms + (uint64_t)ticks * 1000 / 65536 ==
                   (uint64_t)timeout * 1000);
        }
    }
    assert(heartbeat_ms == 15000);
    puts("19 BCM2835 register/deadline cases passed");
    return 0;
}
'''.replace("HEARTBEAT", heartbeat)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--source", type=Path, required=True)
    parser.add_argument("--cc", default="cc")
    args = parser.parse_args()
    with tempfile.TemporaryDirectory(prefix="spacewars-bcm2835-") as directory:
        source = Path(directory) / "fixture.c"
        executable = Path(directory) / "fixture"
        source.write_text(harness(args.source.read_text()))
        subprocess.run([
            *shlex.split(args.cc), "-UNDEBUG", "-std=c11", "-O2", "-Wall", "-Wextra",
            "-Werror", "-Wconversion", "-Wshadow", str(source), "-o", str(executable)
        ], check=True)
        subprocess.run([str(executable)], check=True, timeout=10)


if __name__ == "__main__":
    main()
