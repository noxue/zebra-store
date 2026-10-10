"""Local checks for the optional Nginx domain synchronizer."""

import importlib.util
import io
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch


SCRIPT = Path(__file__).with_name("sync-nginx-reseller-domains.py")
SPEC = importlib.util.spec_from_file_location("reseller_domain_sync", SCRIPT)
assert SPEC and SPEC.loader
sync = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(sync)


class DomainSyncTests(unittest.TestCase):
    def test_certificate_failure_never_routes_unsecured_domain(self):
        with tempfile.TemporaryDirectory() as folder:
            base = Path(folder)
            token = base / "token"
            token.write_text("secret")
            output = base / "resellers.conf"
            argv = [str(SCRIPT), "--token-file", str(token), "--output", str(output), "--email", "owner@example.test"]
            calls = []

            def fake_run(*args):
                calls.append(args)
                if args[0] == "certbot":
                    raise subprocess.CalledProcessError(1, args)

            with patch.object(sys, "argv", argv), patch.object(sync, "urlopen", lambda *args, **kwargs: io.BytesIO(b'{"data":{"domains":["shop.example.test"]}}')), patch.object(sync, "run", fake_run), patch.object(Path, "is_file", return_value=False):
                self.assertEqual(sync.main(), 1)
            self.assertNotIn("server_name", output.read_text())
            self.assertEqual(calls[0][0], "certbot")

    def test_active_domain_is_added_then_removed(self):
        with tempfile.TemporaryDirectory() as folder:
            base = Path(folder)
            token = base / "token"
            token.write_text("secret")
            output = base / "resellers.conf"
            argv = [str(SCRIPT), "--token-file", str(token), "--output", str(output), "--email", "owner@example.test"]
            inventories = iter([["shop.example.test"], []])

            def fake_urlopen(*args, **kwargs):
                return io.BytesIO(json.dumps({"data": {"domains": next(inventories)}}).encode())

            calls = []
            with patch.object(sys, "argv", argv), patch.object(sync, "urlopen", fake_urlopen), patch.object(sync, "run", lambda *args: calls.append(args)), patch.object(Path, "is_file", return_value=True):
                self.assertEqual(sync.main(), 0)
                self.assertIn("server_name shop.example.test;", output.read_text())
                self.assertIn("location ^~ /api/v1/internal/", output.read_text())
                self.assertEqual(sync.main(), 0)
                self.assertNotIn("server_name", output.read_text())
            self.assertEqual(calls.count(("nginx", "-t")), 2)
            self.assertFalse(any(call[0] == "certbot" for call in calls))

    def test_empty_inventory_creates_no_vhosts_or_certificates(self):
        with tempfile.TemporaryDirectory() as folder:
            base = Path(folder)
            token = base / "token"
            token.write_text("secret\n")
            output = base / "resellers.conf"
            argv = [str(SCRIPT), "--token-file", str(token), "--output", str(output), "--email", "owner@example.test"]
            calls = []

            def fake_urlopen(request, timeout):
                self.assertEqual(request.get_header("Authorization"), "Bearer secret")
                self.assertEqual(timeout, 10)
                return io.BytesIO(json.dumps({"data": {"domains": []}}).encode())

            with patch.object(sys, "argv", argv), patch.object(sync, "urlopen", fake_urlopen), patch.object(sync, "run", lambda *args: calls.append(args)):
                self.assertEqual(sync.main(), 0)
                self.assertNotIn("server {", output.read_text())
                self.assertEqual(calls, [("nginx", "-t"), ("systemctl", "reload", "nginx")])
                calls.clear()
                self.assertEqual(sync.main(), 0)
                self.assertEqual(calls, [])

    def test_bad_inventory_never_writes_nginx_config(self):
        with tempfile.TemporaryDirectory() as folder:
            base = Path(folder)
            token = base / "token"
            token.write_text("secret")
            output = base / "resellers.conf"
            argv = [str(SCRIPT), "--token-file", str(token), "--output", str(output), "--email", "owner@example.test"]
            with patch.object(sys, "argv", argv), patch.object(sync, "urlopen", lambda *args, **kwargs: io.BytesIO(b'{"data":{"domains":["bad;domain"]}}')):
                with self.assertRaises(ValueError):
                    sync.main()
            self.assertFalse(output.exists())


if __name__ == "__main__":
    unittest.main()
