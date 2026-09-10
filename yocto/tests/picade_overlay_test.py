import shutil
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "meta-spacewars/lib"))
from spacewars.picade_overlay import validate_picade_overlay


def fixture():
    # Synthetic board fixture, not a copy of the unlicensed upstream overlay.
    pads = [
        ("up", 12, 17, 0xffffffff), ("down", 6, 17, 1),
        ("left", 20, 16, 0xffffffff), ("right", 16, 16, 1),
        ("button1", 5, 304, None), ("button2", 11, 305, None),
        ("button3", 8, 308, None), ("button4", 25, 307, None),
        ("button5", 9, 310, None), ("button6", 10, 311, None),
        ("coin", 23, 314, None), ("start", 24, 315, None),
    ]
    keys = [("enter", 27, 28, None), ("escape", 22, 1, None), ("power", 17, 116, None)]

    def inputs(buttons):
        result = []
        for name, pin, code, value in buttons:
            axis = f"linux,input-type = <3>; linux,input-value = <{value}>;" if value is not None else ""
            result.append(f'{name}: {name} {{ linux,code = <{code}>; gpios = <&gpio {pin} 1>; {axis} }};')
        return "\n".join(result)

    def group(name, buttons):
        return f"""
        {name}: {name} {{
            brcm,pins = <{' '.join(str(button[1]) for button in buttons)}>;
            brcm,function = <{' '.join('0' for _ in buttons)}>;
            brcm,pull = <{' '.join('2' for _ in buttons)}>;
        }};
        """

    overrides = "\n".join(
        f'{name} = <&{name}>, "linux,code:0";'
        for name, *_ in pads + keys if name != "power"
    )
    return f"""
    /dts-v1/;
    /plugin/;
    / {{
        fragment@0 {{
            target = <&gpio>;
            __overlay__ {{
                {group("picade_pins", pads)}
                {group("picade_key_pins", keys)}
            }};
        }};
        fragment@2 {{
            target-path = "/";
            __overlay__ {{
                gpio_keys {{
                    compatible = "gpio-keys-polled";
                    label = "Space-Wars Picade";
                    poll-interval = <4>;
                    pinctrl-0 = <&picade_pins>;
                    {inputs(pads)}
                }};
                picade_keys {{
                    compatible = "gpio-keys";
                    label = "Space-Wars Picade Keys";
                    pinctrl-0 = <&picade_key_pins>;
                    {inputs(keys)}
                }};
            }};
        }};
        __overrides__ {{ {overrides} }};
    }};
    """


class PicadeOverlayTest(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        for tool in ("dtc", "fdtget"):
            if shutil.which(tool) is None:
                raise RuntimeError("Picade tests require device-tree-compiler (dtc and fdtget)")

    def validate(self, source):
        with tempfile.TemporaryDirectory(prefix="spacewars-picade-test-") as directory:
            dts = Path(directory) / "input.dts"
            dtbo = Path(directory) / "input.dtbo"
            dts.write_text(source)
            subprocess.run(["dtc", "-@", "-I", "dts", "-O", "dtb", "-o", str(dtbo), str(dts)],
                           check=True, capture_output=True)
            validate_picade_overlay(dtbo)

    def test_separate_gamepad_and_keyboard_are_valid(self):
        self.validate(fixture())

    def test_utility_keys_inside_gamepad_are_rejected(self):
        source = fixture()
        escape = 'escape: escape { linux,code = <1>; gpios = <&gpio 22 1>;  };'
        source = source.replace(escape, "").replace(
            "pinctrl-0 = <&picade_pins>;", f"pinctrl-0 = <&picade_pins>; {escape}"
        )
        with self.assertRaisesRegex(ValueError, "only its own"):
            self.validate(source)

    def test_keyboard_cannot_use_gamepad_codes(self):
        with self.assertRaisesRegex(ValueError, "escape has the wrong event code"):
            self.validate(fixture().replace("escape { linux,code = <1>", "escape { linux,code = <305>"))

    def test_gamepad_cannot_use_keyboard_codes(self):
        with self.assertRaisesRegex(ValueError, "button1 has the wrong event code"):
            self.validate(fixture().replace("button1 { linux,code = <304>", "button1 { linux,code = <29>"))

    def test_utility_pins_cannot_share_gamepad_pinctrl(self):
        with self.assertRaisesRegex(ValueError, "wrong pinctrl group"):
            self.validate(fixture().replace("pinctrl-0 = <&picade_key_pins>", "pinctrl-0 = <&picade_pins>"))

    def test_split_preserves_the_parameter_targets(self):
        with self.assertRaisesRegex(ValueError, "escape parameter"):
            self.validate(fixture().replace('escape = <&escape>', 'escape = <&enter>'))

    def test_hat_axis_direction_and_polling_are_checked(self):
        with self.assertRaisesRegex(ValueError, "wrong axis value"):
            self.validate(fixture().replace("linux,input-value = <4294967295>", "linux,input-value = <1>", 1))
        with self.assertRaisesRegex(ValueError, "4 ms poll"):
            self.validate(fixture().replace("poll-interval = <4>", "poll-interval = <40>"))


if __name__ == "__main__":
    unittest.main()
