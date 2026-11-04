<!-- Copyright © SixtyFPS GmbH <info@slint.dev> ; SPDX-License-Identifier: GPL-3.0-only OR LicenseRef-Slint-Royalty-free-2.0 OR LicenseRef-Slint-Software-3.0 -->

**NOTE**: This library is an **internal** crate of the [Slint project](https://slint.dev).
This crate should **not be used directly** by applications using Slint.
You should use the `slint` crate instead.

**WARNING**: This crate does not follow the semver convention for versioning and can
only be used with `version = "=x.y.z"` in Cargo.toml.

Space-Wars adds `profiling::presented_frames()` for its local OTA health observer.
It reports completed DRM page flips independently of optional trace/profiling
capture. Requests, rendering and submissions alone do not advance it. The
`health_counts_completed_page_flips_without_optional_profiling` regression covers
that distinction; actual display/watchdog qualification is recorded separately
in `docs/ota-updates.md`.
