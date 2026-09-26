# Settings loading and compatibility

The app reads `settings.toml` from the selected config directory: the desktop
platform config directory, `--config-dir` / `SPACEWARS_CONFIG_DIR` when supplied,
or `/data/spacewars/config` (also mounted at `/var/lib/spacewars`) on the kiosks.

## Recovery contract

- A missing file, section, or field uses its declared default. Existing values
  in other fields are unchanged. Defaults live in `engine-common::Settings`
  and its nested types, not in a second loader schema.
- A rejected value (wrong type or unsupported enum) defaults only that field.
  A wrongly typed section defaults that section. Existing field-specific
  coercion and range-normalization rules still apply.
- A structurally malformed controller-profile record is skipped as a unit;
  valid sibling profiles remain. Do not partially apply a broken binding map.
  The controller layer also validates logical completeness and duplicates.
- Unknown keys and sections are ignored during execution but preserved when
  saving, including nested keys. This is different from an unsupported **value
  of a known field**: `player_2_controller = "future-bot"` is defaulted, whereas
  a future `audio.output_device` setting is retained untouched.
- Invalid UTF-8 or unparseable TOML requires whole-file recovery to defaults.
  Before discarding invalid input, the loader keeps a byte-for-byte
  `settings.toml.bad` backup (then `.bad.1`, etc.). Individual-field recovery
  also keeps a backup. Repeated attempts with identical bytes reuse it.
- Read errors, backup failures, and recovery-limit failures are reported;
  they do not authorize overwriting the original file with defaults.

Startup logs report each recovered field/record path and the backup location,
without logging the rejected field values. Missing values are routine schema
evolution and do not generate per-field warnings. Syntax errors retain the
existing malformed-file diagnostic.

For example, an older build that does not know `value-bot` uses the declared
player-controller default, while keeping the cabinet's 5% volume, autoplay,
renderer, seed, and all other valid preferences. In automatic matches the
normal automatic-player policy still applies to that default controller.

## Saving and cost

All save callers use the same persistence function. It re-reads the existing
document and merges only keys unknown to the current typed settings into the
new snapshot. Known optional fields can be cleared; deleted controller profiles
stay deleted. Profile metadata follows `device_key`, and binding metadata follows
`control`, through edits and reordering. Changing an input-source variant replaces
that variant's fields. Future editable record collections must declare a stable
identity here as well; without one, only unchanged records retain extra keys.

The file is written to a temporary sibling, fsynced, then atomically replaced.
Runtime saves use the existing ordered background writer; there is no extra
per-frame work or filesystem polling. Recovery retries are bounded to 128 bad
fields/records per document. Beyond that, loading/saving returns an error and
leaves the source intact rather than resetting unrelated preferences.

The persistence document uses TOML's parsed representation, not JSON or a signed
integer-only value tree. Full-width `u64` seeds, unknown dates/times, and TOML
non-finite floats survive saves. Formatting/comments are not preserved; keys
and values are. As before, external simultaneous edits of known preferences
are not a supported multi-writer workflow.

## Tests

```sh
cargo test -p engine-client --bin engine-client settings::
cargo test -p engine-client --bin engine-client settings_writer::
# Real app startup, recovery, normal UI save, and persistence/error handling:
xvfb-run -a cargo test -p engine-client --test ui_control_functional sound:: \
  -- --ignored --test-threads=1
```

The settings tests systematically omit the currently serialized settings fields
and sections, comparing against their declared defaults. They also cover the
unsupported-bot regression, multiple invalid fields, unknown nested settings,
optional-value clearing, record deletion/reordering, maximum-width seeds,
syntax/UTF-8 backups, and bounded recovery failure. The sound UI workflow restarts
the real app with an unsupported bot value and confirms both its saved audio
preferences and unknown audio settings survive subsequent UI saves.
