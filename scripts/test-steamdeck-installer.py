"""Exercise installation and failure paths in an isolated Linux user directory."""
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest


ROOT = Path(__file__).resolve().parents[1]
APP = "io.github.ik3rurru.WindowDeck"


class InstallerTest(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="windowdeck installer ")
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.profile = self.root / "user profile"
        self.desktop = self.profile / "Escritorio con espacios"
        self.profile.mkdir()
        self.package = self.root / "download with spaces"
        self.package.mkdir()
        shutil.copy(ROOT / "scripts/install-steamdeck.sh", self.package)
        shutil.copy(ROOT / "packaging/steamdeck/windowdeck", self.package)
        (self.package / "WindowDeck.flatpak").write_bytes(b"test bundle")
        self.location = self.root / "flatpak installation"
        self.export = self.location / f"export/share/applications/{APP}.desktop"
        self.export.parent.mkdir(parents=True)
        self.export.write_text(
            f"[Desktop Entry]\nType=Application\nName=WindowDeck\n"
            f"Exec=/usr/bin/flatpak run {APP} --fullscreen\n"
        )
        self.mock = self.root / "bin"
        self.mock.mkdir()
        self.log = self.root / "flatpak.log"
        self.tool("flatpak", """#!/bin/bash
printf '%s\\n' "$@" >> "$WINDOWDECK_TEST_LOG"
case "$1" in
    install) exit "${WINDOWDECK_TEST_INSTALL_STATUS:-0}" ;;
    info)
        [[ "$2" == "$WINDOWDECK_TEST_SCOPE" ]] || exit 1
        if [[ "${3:-}" == --show-location ]]; then
            printf '%s\\n' "$WINDOWDECK_TEST_LOCATION"
        fi ;;
    run) exit 0 ;;
    *) exit 90 ;;
esac
""")
        self.tool("xdg-user-dir", '#!/bin/sh\nprintf "%s\\n" "$WINDOWDECK_TEST_DESKTOP"\n')
        # Only the child process uses this disposable profile; the parent is unchanged.
        self.env = dict(os.environ, HOME=str(self.profile),
                        PATH=f"{self.mock}{os.pathsep}{os.environ['PATH']}",
                        WINDOWDECK_TEST_LOG=str(self.log),
                        WINDOWDECK_TEST_SCOPE="--user",
                        WINDOWDECK_TEST_LOCATION=str(self.location),
                        WINDOWDECK_TEST_DESKTOP=str(self.desktop))

    def tool(self, name, content):
        path = self.mock / name
        path.write_text(content)
        path.chmod(0o755)

    def install(self, *args, success=True):
        result = subprocess.run(["bash", str(self.package / "install-steamdeck.sh"), *args],
                                cwd=self.root, env=self.env, text=True, capture_output=True)
        if success:
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        else:
            self.assertNotEqual(result.returncode, 0)
        return result

    def assert_no_shortcuts(self):
        self.assertFalse((self.profile / ".local/bin/windowdeck").exists())
        self.assertFalse(self.desktop.exists())

    def test_install_localized_desktop_and_update_in_place(self):
        self.install()
        entry = self.desktop / "WindowDeck.desktop"
        self.assertEqual(entry.read_bytes(), self.export.read_bytes())
        self.assertTrue(os.access(entry, os.X_OK))
        launcher = self.profile / ".local/bin/windowdeck"
        self.assertTrue(os.access(launcher, os.X_OK))
        self.install()
        self.assertEqual(list(self.desktop.iterdir()), [entry])
        calls = self.log.read_text()
        self.assertIn(f"install\n--user\n--bundle\n{self.package}/WindowDeck.flatpak\n", calls)

    def test_cancelled_install_does_not_create_shortcuts(self):
        self.env["WINDOWDECK_TEST_INSTALL_STATUS"] = "1"
        self.install(success=False)
        self.assert_no_shortcuts()

    def test_missing_bundle_does_not_invoke_flatpak(self):
        self.install("absent.flatpak", success=False)
        self.assert_no_shortcuts()
        self.assertFalse(self.log.exists())

    def test_shortcuts_for_system_install_do_not_reinstall(self):
        self.env["WINDOWDECK_TEST_SCOPE"] = "--system"
        self.install("--shortcuts-only")
        self.assertTrue((self.desktop / "WindowDeck.desktop").exists())
        self.assertNotIn("install\n", self.log.read_text())

    def test_missing_application_does_not_create_shortcuts(self):
        self.env["WINDOWDECK_TEST_SCOPE"] = "none"
        self.install("--shortcuts-only", success=False)
        self.assert_no_shortcuts()

    def test_disabled_desktop_still_installs_steam_launcher(self):
        self.env["WINDOWDECK_TEST_DESKTOP"] = str(self.profile)
        self.install()
        self.assertTrue((self.profile / ".local/bin/windowdeck").exists())
        self.assertFalse((self.profile / "WindowDeck.desktop").exists())
        self.assertFalse(self.desktop.exists())

    def test_launcher_forwards_arguments_without_shell_expansion(self):
        self.install()
        self.log.write_text("")
        result = subprocess.run([str(self.profile / ".local/bin/windowdeck"),
                                 "192.0.2.1:48150", "--native", "literal $argument"],
                                env=self.env)
        self.assertEqual(result.returncode, 0)
        self.assertEqual(self.log.read_text().splitlines(),
                         ["run", APP, "192.0.2.1:48150", "--native", "literal $argument", "--fullscreen"])


if __name__ == "__main__":
    unittest.main()
