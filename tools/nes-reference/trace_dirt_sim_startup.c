// Minimal DirtSim host used by startup_trace.gdb. Not part of the Cargo build.

#include "core/scenarios/nes/SmolnesRuntimeBackend.h"

#include <stdio.h>

int main(int argc, char** argv)
{
    if (argc != 2) {
        fprintf(stderr, "usage: %s ROM\n", argv[0]);
        return 2;
    }
    SmolnesRuntimeHandle* runtime = smolnesRuntimeCreate();
    if (runtime == NULL) {
        return 1;
    }
    smolnesRuntimeSetApuEnabled(runtime, false);
    smolnesRuntimeSetDetailedTimingEnabled(runtime, false);
    smolnesRuntimeSetPixelOutputEnabled(runtime, false);
    const int ok = smolnesRuntimeStart(runtime, argv[1])
        && smolnesRuntimeRunFrames(runtime, 1, 10000);
    if (!ok) {
        char error[256];
        smolnesRuntimeGetLastErrorCopy(runtime, error, sizeof(error));
        fprintf(stderr, "%s\n", error);
    }
    smolnesRuntimeStop(runtime);
    smolnesRuntimeDestroy(runtime);
    return ok ? 0 : 1;
}
