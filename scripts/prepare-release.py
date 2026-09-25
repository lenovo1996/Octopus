"""Stamp MAJOR.MINOR.RUN_NUMBER into an isolated CI checkout, never commit it."""

import argparse
import json
import re
import tomllib
from pathlib import Path


def prepare_release(root: Path, run_number: str) -> str:
    if not re.fullmatch(r"[1-9][0-9]*", run_number):
        raise ValueError("run number must be a positive integer")

    package_path = root / "package.json"
    config_path = root / "src-tauri/tauri.conf.json"
    cargo_path = root / "src-tauri/Cargo.toml"
    lock_path = root / "src-tauri/Cargo.lock"
    package = json.loads(package_path.read_text())
    config = json.loads(config_path.read_text())
    cargo_text = cargo_path.read_text()
    lock_text = lock_path.read_text()
    cargo = tomllib.loads(cargo_text)
    lock = tomllib.loads(lock_text)
    own_packages = [p for p in lock["package"] if p["name"] == "octopus" and "source" not in p]
    if len(own_packages) != 1 or cargo["package"]["name"] != "octopus":
        raise ValueError("expected exactly one local octopus Cargo package")
    base = package["version"]
    if not re.fullmatch(r"(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)", base):
        raise ValueError("source version must be a stable MAJOR.MINOR.PATCH version")
    if any(version != base for version in [config["version"], cargo["package"]["version"], own_packages[0]["version"]]):
        raise ValueError("package.json, Tauri, Cargo.toml and Cargo.lock versions must match")
    major, minor, _patch = base.split(".")
    version = f"{major}.{minor}.{run_number}"

    def replace_version(text: str, pattern: str) -> str:
        result, count = re.subn(pattern, lambda m: m[1] + version + m[2], text, flags=re.MULTILINE)
        if count != 1:
            raise ValueError("expected exactly one application version to stamp")
        return result

    cargo_text = replace_version(cargo_text, r'(\[package\][\s\S]*?^version\s*=\s*")[^"]+(".*)')
    lock_text = replace_version(lock_text, r'(\[\[package\]\]\nname = "octopus"\nversion = ")[^"]+(".*)')
    package["version"] = config["version"] = version
    # Validate every input before writing; dependency versions stay locked.
    package_path.write_text(json.dumps(package, indent=2, ensure_ascii=False) + "\n")
    config_path.write_text(json.dumps(config, indent=2, ensure_ascii=False) + "\n")
    cargo_path.write_text(cargo_text)
    lock_path.write_text(lock_text)
    return version


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("run_number")
    parser.add_argument("--root", type=Path, default=Path.cwd())
    args = parser.parse_args()
    print(prepare_release(args.root, args.run_number))
