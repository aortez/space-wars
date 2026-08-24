# Functional UI tests

The functional suite launches the real `engine-client` binary and controls it
only through the public Unix-socket API in `spacewars-control`. It protects the
process, protocol, Slint callback, menu-navigation, rendering, and screenshot
boundaries that unit tests cannot cover together.

Each test owns:

- a fresh `engine-client` process;
- an isolated temporary settings directory;
- a unique short control-socket path;
- deterministic seed `4242`;
- the Winit software renderer;
- readiness and transition polling with explicit deadlines; and
- a child guard that always terminates and reaps the process.

The initial workflows verify:

- launcher state, inventory, accepted actions, and reachability of every menu
  choice;
- opening, changing, and closing Spacewars Settings;
- opening Controls, entering and leaving Touch Test, and returning to the
  launcher;
- wrong-screen, stale-revision, and unavailable-action rejections without state
  mutation;
- launcher and gameplay screenshots; and
- starting a real deterministic Spacewars scenario and observing `gameplay`
  with a scenario instance revision.

These are semantic UI tests. They do not validate physical touchscreen hit
testing, LinuxKMS coordinate transforms, or panel rotation.

## Run locally

On Debian or Ubuntu, install the virtual display tools once:

```sh
sudo apt-get install xvfb xauth
```

Run the suite under an isolated X display:

```sh
xvfb-run -a -s "-screen 0 1280x1024x24" \
  cargo test -p engine-client --test ui_control_functional -- \
  --ignored --test-threads=1
```

To watch the workflows on an existing X display, omit `xvfb-run`:

```sh
cargo test -p engine-client --test ui_control_functional -- \
  --ignored --test-threads=1 --nocapture
```

The tests are marked ignored so the ordinary cross-platform workspace command
does not require a display. Linux CI runs them explicitly under Xvfb.

## Failure artifacts

Successful runs remove their temporary data. A failing workflow preserves a
directory under `target/functional-test-artifacts/` containing as much of the
following as the still-running client can provide:

```text
engine-client.log
failure.png
last-state.json
command-history.json
summary.json
```

CI uploads that directory when the functional step fails. `summary.json` and
`command-history.json` are versioned JSON; test and engine diagnostics remain on
stderr or in the log, keeping control output out of stdout.

## Adding workflows

Keep functional tests black-box:

1. Start a fresh client and wait for readiness.
2. Assert the initial state before acting.
3. Drive only `ControlClient` operations available to ordinary tooling.
4. Guard mutations with screen/revision preconditions.
5. Wait on observable predicates with deadlines; do not use transition sleeps.
6. Assert both the postcondition and state that must remain unchanged on errors.

Scenario pause, restart, and return-to-launcher coverage will be added when
their explicit public lifecycle operations land. The current API deliberately
does not reinterpret menu actions as gameplay controls.
