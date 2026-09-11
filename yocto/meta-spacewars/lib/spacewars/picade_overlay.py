"""Validate the compiled Picade input layout before deploying its overlay."""

import subprocess


def validate_picade_overlay(overlay, fdtget="fdtget"):
    def read(node, prop=None, kind="u", option=None):
        args = [str(fdtget)]
        args += [option] if option else ["-t", kind]
        args += [str(overlay), node]
        if prop is not None:
            args.append(prop)
        try:
            return subprocess.check_output(args, text=True, stderr=subprocess.PIPE).strip()
        except (OSError, subprocess.CalledProcessError) as error:
            raise ValueError(f"Cannot read Picade overlay {node} {prop or ''}: {error}") from error

    def cells(node, prop):
        return [int(value) for value in read(node, prop).split()]

    def expect(condition, message):
        if not condition:
            raise ValueError(f"Invalid Picade input layout: {message}")

    root = "/fragment@2/__overlay__"
    gamepad = f"{root}/gpio_keys"
    keyboard = f"{root}/picade_keys"
    # name: (GPIO, event code, input type, axis press value)
    gamepad_inputs = {
        "up": (12, 17, 3, 0xffffffff),
        "down": (6, 17, 3, 1),
        "left": (20, 16, 3, 0xffffffff),
        "right": (16, 16, 3, 1),
        "button1": (5, 304, 1, None),
        "button2": (11, 305, 1, None),
        "button3": (8, 308, 1, None),
        "button4": (25, 307, 1, None),
        "button5": (9, 310, 1, None),
        "button6": (10, 311, 1, None),
        "coin": (23, 314, 1, None),
        "start": (24, 315, 1, None),
    }
    keyboard_inputs = {
        "enter": (27, 28, 1, None),
        "escape": (22, 1, 1, None),
        "power": (17, 116, 1, None),
    }
    devices = [
        (gamepad, "Space-Wars Picade", "gpio-keys-polled", "picade_pins", gamepad_inputs),
        (keyboard, "Space-Wars Picade Keys", "gpio-keys", "picade_key_pins", keyboard_inputs),
    ]
    used_pins = set()
    for node, label, driver, pin_group, inputs in devices:
        expect(read(node, "label", kind="s") == label, f"{node} must identify as {label}")
        expect(read(node, "compatible", kind="s") == driver, f"{label} must use {driver}")
        expect(set(read(node, option="-l").split()) == set(inputs),
               f"{label} must contain only its own gamepad or utility inputs")
        pins = []
        for name, (pin, code, input_type, value) in inputs.items():
            child = f"{node}/{name}"
            expect(cells(child, "linux,code") == [code], f"{name} has the wrong event code")
            properties = set(read(child, option="-p").split())
            actual_type = cells(child, "linux,input-type") if "linux,input-type" in properties else [1]
            expect(actual_type == [input_type], f"{name} has the wrong input type")
            if value is not None:
                expect(cells(child, "linux,input-value") == [value], f"{name} has the wrong axis value")
            gpio = cells(child, "gpios")
            expect(len(gpio) == 3 and gpio[1:] == [pin, 1], f"{name} must use active-low GPIO {pin}")
            expect(pin not in used_pins, f"GPIO {pin} is assigned to more than one input")
            used_pins.add(pin)
            pins.append(pin)
            if name != "power":
                override = bytes(int(value, 16) for value in read("/__overrides__", name, kind="bx").split())
                expect(int.from_bytes(override[:4], "big") == cells(child, "phandle")[0]
                       and override[4:] == b"linux,code:0\0",
                       f"{name} parameter must still target its input after the split")
        group = f"/fragment@0/__overlay__/{pin_group}"
        expect(cells(node, "pinctrl-0") == cells(group, "phandle"), f"{label} has the wrong pinctrl group")
        expect(sorted(cells(group, "brcm,pins")) == sorted(pins), f"{label} must own only its GPIO pins")
        expect(cells(group, "brcm,function") == [0] * len(pins), f"{label} pins must be inputs")
        expect(cells(group, "brcm,pull") == [2] * len(pins), f"{label} pins must have pull-ups")
    expect(cells(gamepad, "poll-interval") == [4], "gamepad must retain its 4 ms poll interval")
