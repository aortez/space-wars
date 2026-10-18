"""Verify that boot validation cannot accidentally borrow utilities from the host."""

import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "meta-spacewars/lib"))
from spacewars.recovery_rootfs import resolve_in_root, verify_programs


class RecoveryRootfsTest(unittest.TestCase):
    def test_usrmerge_and_absolute_links_stay_within_the_image(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "usr/bin").mkdir(parents=True)
            (root / "usr/bin/tool").write_bytes(b"fixture")
            (root / "bin").symlink_to("usr/bin")
            (root / "usr/bin/mount").symlink_to("/usr/bin/tool")
            self.assertEqual(resolve_in_root(root, "/bin/mount"), root / "usr/bin/tool")
            (root / "usr/bin/mount").unlink()
            # /bin/mount exists on the host; it must not satisfy this dependency.
            with self.assertRaisesRegex(ValueError, "missing initramfs dependency"):
                resolve_in_root(root, "/bin/mount")

    def test_link_cycles_and_parent_escape_are_rejected(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "loop").symlink_to("/loop")
            (root / "escape").symlink_to("../bin/sh")
            for name in ("/loop", "/escape"):
                with self.subTest(name=name), self.assertRaises(ValueError):
                    resolve_in_root(root, name)

    def test_actual_elf_dependency_closure_and_missing_library(self):
        # A small freestanding executable and shared library need no host libc.
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "usr/bin").mkdir(parents=True)
            (root / "usr/lib").mkdir()
            source = root / "probe.c"
            source.write_text("extern int fixture(void); int _start(void) { return fixture(); }\n")
            library_source = root / "library.c"
            library_source.write_text("int fixture(void) { return 0; }\n")
            library = root / "usr/lib/libfixture.so"
            executable = root / "usr/bin/probe"
            subprocess.run(["gcc", "-nostdlib", "-shared", "-fPIC", str(library_source),
                            "-Wl,-soname,libfixture.so", "-o", str(library)], check=True)
            subprocess.run(["gcc", "-nostdlib", str(source), "-L" + str(library.parent),
                            "-Wl,-rpath,$ORIGIN/../lib", "-Wl,--no-dynamic-linker", "-lfixture",
                            "-o", str(executable)], check=True)
            for file in (library, executable):
                file.chmod(0o755)
            machine = int.from_bytes(executable.read_bytes()[18:20], "little")
            options = dict(machine=machine, owner=os.getuid())
            checked = verify_programs(root, ("/usr/bin/probe",), **options)
            self.assertEqual(checked, ["/usr/bin/probe", "/usr/lib/libfixture.so"])
            library.chmod(0o777)
            with self.assertRaisesRegex(ValueError, "unsafe initramfs dependency"):
                verify_programs(root, ("/usr/bin/probe",), **options)
            library.unlink()
            with self.assertRaisesRegex(ValueError, "missing initramfs library"):
                verify_programs(root, ("/usr/bin/probe",), **options)

    def test_missing_script_interpreter_cannot_use_host_shell(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            script = root / "init"
            script.write_text("#!/bin/sh\nexit 0\n")
            script.chmod(0o755)
            with self.assertRaisesRegex(ValueError, "missing initramfs dependency /bin/sh"):
                verify_programs(root, ("/init",), owner=os.getuid())


if __name__ == "__main__":
    unittest.main()
