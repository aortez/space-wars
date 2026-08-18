// Optional reference-capture utility. This file is not part of the Cargo
// workspace and is compiled only against a separately checked-out DirtSim.

#include "core/scenarios/nes/SmolnesRuntimeBackend.h"

#include <inttypes.h>
#include <stdbool.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>

static uint64_t fnv1a(const void* data, size_t size)
{
    const uint8_t* bytes = data;
    uint64_t hash = UINT64_C(14695981039346656037);
    for (size_t i = 0; i < size; ++i) {
        hash ^= bytes[i];
        hash *= UINT64_C(1099511628211);
    }
    return hash;
}

static double monotonicMs(void)
{
    struct timespec now;
    clock_gettime(CLOCK_MONOTONIC, &now);
    return (double)now.tv_sec * 1000.0 + (double)now.tv_nsec / 1000000.0;
}

static bool printSnapshot(SmolnesRuntimeHandle* runtime, const char* label)
{
    uint8_t frame[SMOLNES_RUNTIME_FRAME_BYTES];
    uint8_t palette[SMOLNES_RUNTIME_PALETTE_FRAME_BYTES];
    uint8_t cpuRam[SMOLNES_RUNTIME_CPU_RAM_BYTES];
    uint8_t prgRam[SMOLNES_RUNTIME_PRG_RAM_BYTES];
    SmolnesRuntimePpuSnapshot ppu;
    SmolnesRuntimeControllerSnapshot controller;
    const uint32_t savestateSize = smolnesRuntimeGetSavestateSize();
    uint8_t* savestate = malloc(savestateSize);
    uint64_t frameId = 0;
    uint64_t savestateFrameId = 0;

    if (savestate == NULL
        || !smolnesRuntimeCopyLatestFrame(runtime, frame, sizeof(frame), &frameId)
        || !smolnesRuntimeCopyLatestPaletteIndices(
            runtime, palette, sizeof(palette), NULL)
        || !smolnesRuntimeCopyMemorySnapshot(
            runtime,
            cpuRam,
            sizeof(cpuRam),
            prgRam,
            sizeof(prgRam),
            NULL)
        || !smolnesRuntimeCopyPpuSnapshot(runtime, &ppu)
        || !smolnesRuntimeCopyControllerSnapshot(runtime, &controller)
        || !smolnesRuntimeCopySavestate(
            runtime, savestate, savestateSize, &savestateFrameId)) {
        free(savestate);
        return false;
    }

    printf(
        "snapshot label=%s frame=%" PRIu64
        " rgb565_fnv=%016" PRIx64
        " palette_fnv=%016" PRIx64
        " cpu_ram_fnv=%016" PRIx64
        " prg_ram_fnv=%016" PRIx64
        " ppu_fnv=%016" PRIx64
        " savestate_fnv=%016" PRIx64
        " controller=%02x controller_seq=%" PRIu64
        " controller_frame=%" PRIu64 "\n",
        label,
        frameId,
        fnv1a(frame, sizeof(frame)),
        fnv1a(palette, sizeof(palette)),
        fnv1a(cpuRam, sizeof(cpuRam)),
        fnv1a(prgRam, sizeof(prgRam)),
        fnv1a(&ppu, sizeof(ppu)),
        fnv1a(savestate, savestateSize),
        controller.controller1_state,
        controller.controller1_sequence_id,
        controller.controller1_applied_frame_id);

    free(savestate);
    return frameId == savestateFrameId;
}

static bool captureAtFrame(const char* romPath, uint32_t frames, const char* label)
{
    SmolnesRuntimeHandle* runtime = smolnesRuntimeCreate();
    if (runtime == NULL) {
        return false;
    }
    smolnesRuntimeSetDetailedTimingEnabled(runtime, false);
    const bool ok = smolnesRuntimeStart(runtime, romPath)
        && smolnesRuntimeRunFrames(runtime, frames, 10000)
        && printSnapshot(runtime, label);
    if (!ok) {
        char error[256];
        smolnesRuntimeGetLastErrorCopy(runtime, error, sizeof(error));
        fprintf(stderr, "capture failed (%s): %s\n", label, error);
    }
    smolnesRuntimeStop(runtime);
    smolnesRuntimeDestroy(runtime);
    return ok;
}

static bool captureInputScript(const char* romPath)
{
    SmolnesRuntimeHandle* runtime = smolnesRuntimeCreate();
    if (runtime == NULL) {
        return false;
    }
    smolnesRuntimeSetDetailedTimingEnabled(runtime, false);
    bool ok = smolnesRuntimeStart(runtime, romPath);
    ok = ok && smolnesRuntimeRunFrames(runtime, 100, 10000);
    ok = ok && printSnapshot(runtime, "script_idle_100");

    smolnesRuntimeSetController1State(runtime, SMOLNES_RUNTIME_BUTTON_START);
    ok = ok && smolnesRuntimeRunFrames(runtime, 1, 10000);
    ok = ok && printSnapshot(runtime, "script_start_down_101");
    smolnesRuntimeSetController1State(runtime, 0);
    ok = ok && smolnesRuntimeRunFrames(runtime, 29, 10000);
    ok = ok && printSnapshot(runtime, "script_start_up_130");

    smolnesRuntimeSetController1State(runtime, SMOLNES_RUNTIME_BUTTON_RIGHT);
    ok = ok && smolnesRuntimeRunFrames(runtime, 20, 10000);
    ok = ok && printSnapshot(runtime, "script_right_150");
    smolnesRuntimeSetController1State(runtime, 0);
    ok = ok && smolnesRuntimeRunFrames(runtime, 20, 10000);
    ok = ok && printSnapshot(runtime, "script_release_170");

    if (!ok) {
        char error[256];
        smolnesRuntimeGetLastErrorCopy(runtime, error, sizeof(error));
        fprintf(stderr, "script capture failed: %s\n", error);
    }
    smolnesRuntimeStop(runtime);
    smolnesRuntimeDestroy(runtime);
    return ok;
}

static bool benchmarkMode(
    const char* romPath,
    const char* label,
    bool apu,
    bool pixels,
    bool rgb565)
{
    const uint32_t warmupFrames = 100;
    const uint32_t measuredFrames = 1000;
    SmolnesRuntimeHandle* runtime = smolnesRuntimeCreate();
    if (runtime == NULL) {
        return false;
    }
    smolnesRuntimeSetDetailedTimingEnabled(runtime, false);
    smolnesRuntimeSetApuEnabled(runtime, apu);
    smolnesRuntimeSetPixelOutputEnabled(runtime, pixels);
    smolnesRuntimeSetRgbaOutputEnabled(runtime, rgb565);
    bool ok = smolnesRuntimeStart(runtime, romPath)
        && smolnesRuntimeRunFrames(runtime, warmupFrames, 10000);
    const double started = monotonicMs();
    ok = ok && smolnesRuntimeRunFrames(runtime, measuredFrames, 10000);
    const double elapsedMs = monotonicMs() - started;
    if (ok) {
        printf(
            "benchmark label=%s frames=%u elapsed_ms=%.3f fps=%.3f"
            " apu=%u pixels=%u rgb565=%u\n",
            label,
            measuredFrames,
            elapsedMs,
            (double)measuredFrames * 1000.0 / elapsedMs,
            apu,
            pixels,
            rgb565);
    } else {
        char error[256];
        smolnesRuntimeGetLastErrorCopy(runtime, error, sizeof(error));
        fprintf(stderr, "benchmark failed (%s): %s\n", label, error);
    }
    smolnesRuntimeStop(runtime);
    smolnesRuntimeDestroy(runtime);
    return ok;
}

int main(int argc, char** argv)
{
    if (argc != 2) {
        fprintf(stderr, "usage: %s ROM\n", argv[0]);
        return 2;
    }

    const char* romPath = argv[1];
    bool ok = captureAtFrame(romPath, 1, "idle_1");
    ok = captureAtFrame(romPath, 2, "idle_2") && ok;
    ok = captureAtFrame(romPath, 10, "idle_10") && ok;
    ok = captureAtFrame(romPath, 100, "idle_100") && ok;
    ok = captureInputScript(romPath) && ok;
    ok = benchmarkMode(romPath, "full", true, true, true) && ok;
    ok = benchmarkMode(romPath, "palette_no_apu", false, true, false) && ok;
    ok = benchmarkMode(romPath, "headless_no_apu", false, false, false) && ok;
    return ok ? 0 : 1;
}
