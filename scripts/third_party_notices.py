#!/usr/bin/env python3
"""Generate THIRD_PARTY_NOTICES.md for the shipped Switchboard binaries.

Sources, all offline-reproducible from the lockfiles:
- Rust: `cargo metadata --locked --filter-platform <target>` for each release target,
  walked from the workspace binaries (`fabric-switchboard`, `switchboard-cli`) along
  normal and build edges. Dev-dependencies are test-only and are not shipped.
  Each crate's declared `license` and the license/notice files inside its published
  package (the cargo registry source directory) are read.
- JavaScript: the non-dev packages in package-lock.json, read from node_modules
  (run `npm ci` first), plus Vite's module-preload helper, which Vite injects into
  the production bundle.
- Canonical license texts: scripts/licenses/<SPDX id>.txt, vendored verbatim from
  spdx/license-list-data v3.29.0.

Usage:
  python3 scripts/third_party_notices.py          # rewrite THIRD_PARTY_NOTICES.md
  python3 scripts/third_party_notices.py --check  # exit 1 if the file is stale
"""
from __future__ import annotations

import argparse
import hashlib
import json
import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
OUTPUT = ROOT / "THIRD_PARTY_NOTICES.md"
LICENSE_DIR = ROOT / "scripts" / "licenses"
SPDX_DATA = "spdx/license-list-data v3.29.0"
# Release targets: the macOS universal build is aarch64 + x86_64; Windows is x64 MSVC.
TARGETS = {
    "aarch64-apple-darwin": "macOS",
    "x86_64-apple-darwin": "macOS",
    "x86_64-pc-windows-msvc": "Windows",
}
LICENSE_FILE = re.compile(r"(?i)^(licen[cs]e|copying|notice|copyright|unlicense)")
SPDX_TOKEN = re.compile(r"[A-Za-z0-9][A-Za-z0-9.+-]*")
OPERATORS = {"AND", "OR", "WITH"}
APACHE_MARKERS = (
    "apache license",
    "version 2.0, january 2004",
    "terms and conditions for use, reproduction, and distribution",
)
COPYRIGHT_LINE = re.compile(r"(?i)^\s*copyright\s*(\(c\)|©|\d|\[yyyy\]|\{yyyy\})")
PLACEHOLDER = re.compile(r"(?i)\[yyyy\]|\{yyyy\}|\[name of copyright owner\]|\{name of copyright owner\}")


class NoticeError(Exception):
    pass


def cargo_metadata(target: str) -> dict:
    command = [
        "cargo", "metadata", "--format-version", "1", "--locked",
        "--filter-platform", target,
    ]
    try:
        result = subprocess.run(
            command, cwd=ROOT, check=True, capture_output=True, text=True
        )
    except FileNotFoundError as error:
        raise NoticeError("cargo is not installed; it is required to read Cargo.lock") from error
    except subprocess.CalledProcessError as error:
        raise NoticeError(
            f"`{' '.join(command)}` failed with exit {error.returncode}:\n{error.stderr.strip()}"
        ) from error
    return json.loads(result.stdout)


def shipped_crates() -> dict[tuple[str, str], dict]:
    """Return {(name, version): {package, platforms, linked}} for registry crates."""
    crates: dict[tuple[str, str], dict] = {}
    for target, platform in TARGETS.items():
        meta = cargo_metadata(target)
        packages = {p["id"]: p for p in meta["packages"]}
        nodes = {n["id"]: n for n in meta["resolve"]["nodes"]}
        roots = [
            p["id"] for p in meta["packages"]
            if p["id"] in meta["workspace_members"]
            and any("bin" in t["kind"] for t in p["targets"])
        ]
        if not roots:
            raise NoticeError(f"{target}: no workspace binary found")
        # reach: package id -> True if linked into a binary, False if build-time only.
        reach: dict[str, bool] = {}
        stack = [(root, True) for root in roots]
        while stack:
            package_id, linked = stack.pop()
            if reach.get(package_id) is True or (package_id in reach and not linked):
                continue
            reach[package_id] = linked
            package = packages[package_id]
            is_proc_macro = any("proc-macro" in t["kind"] for t in package["targets"])
            for dep in nodes[package_id]["deps"]:
                kinds = {k["kind"] for k in dep["dep_kinds"]}
                if None in kinds:
                    stack.append((dep["pkg"], linked and not is_proc_macro))
                if "build" in kinds:
                    stack.append((dep["pkg"], False))
        for package_id, linked in reach.items():
            package = packages[package_id]
            if package["source"] is None:
                continue  # our own workspace crates
            if not package["source"].startswith("registry+"):
                raise NoticeError(f"{package['name']}: non-registry source {package['source']}")
            if any("proc-macro" in t["kind"] for t in package["targets"]):
                linked = False
            key = (package["name"], package["version"])
            entry = crates.setdefault(
                key, {"package": package, "platforms": set(), "linked": False}
            )
            entry["platforms"].add(platform)
            entry["linked"] = entry["linked"] or linked
    return crates


def read_text(path: Path) -> str:
    raw = path.read_bytes()
    try:
        text = raw.decode("utf-8")
    except UnicodeDecodeError:
        text = raw.decode("latin-1")
    text = text.replace("\r\n", "\n").replace("\r", "\n").lstrip("﻿")
    lines = [line.rstrip() for line in text.split("\n")]
    while lines and not lines[0]:
        lines.pop(0)
    while lines and not lines[-1]:
        lines.pop()
    return "\n".join(lines)


def is_plain_apache(text: str) -> bool:
    """A full Apache-2.0 text with no filled-in copyright: the canonical text covers it."""
    folded = " ".join(text.lower().split())
    if not all(marker in folded for marker in APACHE_MARKERS):
        return False
    for line in text.split("\n"):
        # A copyright statement, not a wrapped line of the terms ("copyright notice ...").
        if COPYRIGHT_LINE.match(line) and not PLACEHOLDER.search(line):
            return False
    return True


def license_ids(expression: str) -> list[str]:
    normalized = expression.replace("/", " OR ")
    return [t for t in SPDX_TOKEN.findall(normalized) if t not in OPERATORS]


def fence(text: str) -> str:
    longest = max((len(m) for m in re.findall(r"`+", text)), default=0)
    return "`" * max(3, longest + 1)


def cell(text: str) -> str:
    return text.replace("|", "\\|")


def collect_files(directory: Path) -> list[Path]:
    return sorted(
        (p for p in directory.iterdir() if p.is_file() and LICENSE_FILE.match(p.name)),
        key=lambda p: p.name.lower(),
    )


def build() -> str:
    crates = shipped_crates()

    components = []  # dicts: kind, name, version, license, use, platforms, files, url
    for (name, version), entry in sorted(crates.items()):
        package = entry["package"]
        directory = Path(package["manifest_path"]).parent
        expression = package.get("license") or ""
        files = collect_files(directory)
        declared_file = package.get("license_file")
        if declared_file:
            candidate = (directory / declared_file).resolve()
            if candidate.is_file() and candidate not in [f.resolve() for f in files]:
                files.append(candidate)
        components.append({
            "kind": "rust",
            "name": name,
            "version": version,
            "license": expression,
            "use": "linked" if entry["linked"] else "build-time",
            "platforms": ", ".join(sorted(entry["platforms"])),
            "files": files,
            "base": directory,
            "url": f"https://crates.io/crates/{name}/{version}",
        })

    lock = json.loads((ROOT / "package-lock.json").read_text(encoding='utf-8'))
    node_modules = ROOT / "node_modules"
    for path, info in sorted(lock["packages"].items()):
        if not path or info.get("dev") or info.get("devOptional"):
            continue
        directory = ROOT / path
        if not directory.is_dir():
            raise NoticeError(f"{path} is missing; run `npm ci` first")
        name = path.split("node_modules/")[-1]
        components.append({
            "kind": "js",
            "name": name,
            "version": info["version"],
            "license": info.get("license") or "",
            "use": "bundled into the desktop UI",
            "platforms": "macOS, Windows",
            "files": collect_files(directory),
            "base": directory,
            "url": f"https://www.npmjs.com/package/{name}/v/{info['version']}",
        })
    vite = lock["packages"].get("node_modules/vite")
    if not vite or not (node_modules / "vite" / "LICENSE.md").is_file():
        raise NoticeError("node_modules/vite/LICENSE.md is missing; run `npm ci` first")
    vite_core = read_text(node_modules / "vite" / "LICENSE.md").split(
        "# Licenses of bundled dependencies"
    )[0].strip()
    components.append({
        "kind": "js",
        "name": "vite (module-preload helper only)",
        "version": vite["version"],
        "license": vite.get("license") or "MIT",
        "use": "bundled into the desktop UI",
        "platforms": "macOS, Windows",
        "files": [],
        "inline": vite_core,
        "base": node_modules / "vite",
        "url": f"https://www.npmjs.com/package/vite/v/{vite['version']}",
    })

    # Unique license/notice texts, in order of first use.
    texts: dict[str, dict] = {}
    unreadable = []
    for component in components:
        label = f"{component['name']} {component['version']}"
        component["text_refs"] = []
        bodies = [(f.name, read_text(f)) for f in component["files"]]
        if component.get("inline"):
            bodies.append(("LICENSE.md (Vite core section)", component["inline"]))
        for file_name, body in bodies:
            if not body or is_plain_apache(body):
                continue
            digest = hashlib.sha256(" ".join(body.split()).encode()).hexdigest()
            slot = texts.setdefault(digest, {"body": body, "users": []})
            slot["users"].append(f"{label} ({file_name})")
            component["text_refs"].append(digest)
        if not component["license"]:
            unreadable.append(label)

    ids = sorted({i for c in components for i in license_ids(c["license"])})
    missing = [i for i in ids if not (LICENSE_DIR / f"{i}.txt").is_file()]
    if missing:
        raise NoticeError(
            "no vendored license text for: " + ", ".join(missing)
            + f" — add scripts/licenses/<id>.txt from {SPDX_DATA}"
        )
    numbers = {digest: n for n, digest in enumerate(texts, 1)}

    rust = [c for c in components if c["kind"] == "rust"]
    js = [c for c in components if c["kind"] == "js"]
    no_files = [c for c in components if not c["files"] and not c.get("inline")]
    mpl = [c for c in components if "MPL-2.0" in license_ids(c["license"])]

    out = []
    add = out.append
    add("# Third-party notices")
    add("")
    add("The Switchboard desktop app (`Fabric Switchboard`) and the `switchboard` CLI, as")
    add("built for macOS (universal) and Windows (x64), include the third-party components")
    add("listed below. Each component stays under its own license, shown here. The")
    add("Switchboard [LICENSE](LICENSE) covers Switchboard's own code only and does not")
    add("change the license of any component.")
    add("")
    add("This file is generated by `python3 scripts/third_party_notices.py` from")
    add("`Cargo.lock` (through `cargo metadata --locked` for each release target) and")
    add("`package-lock.json`, reading each component's declared license and the license and")
    add("notice files in its published package. `./scripts/check.sh` fails when it is stale.")
    add("Do not edit it by hand.")
    add("")
    linked = sum(c["use"] == "linked" for c in rust)
    add(f"- Rust crates: {len(rust)} ({linked} linked, {len(rust) - linked} build-time only)")
    add(f"- JavaScript packages bundled into the desktop UI: {len(js)}")
    add(f"- Distinct license and notice texts reproduced: {len(texts)}")
    add(f"- Components whose license could not be read: {len(unreadable)}"
        + (f" ({', '.join(unreadable)})" if unreadable else ""))
    add("")
    add("Where a component offers a choice of licenses (an `OR` expression, or `/` in older")
    add("manifests), every offered license is listed and its text is included; the component")
    add("is used under the terms of any one of them. **Use** is `linked` for code compiled")
    add("into the binaries and `build-time` for build scripts and procedural macros, which")
    add("run while compiling and are listed because code they generate can end up in the")
    add("binaries.")
    add("")
    add("## Rust crates")
    add("")
    add("| Crate | Version | License | Use | Platforms | Texts |")
    add("|---|---|---|---|---|---|")
    for c in rust:
        refs = ", ".join(f"[{numbers[d]}](#text-{numbers[d]})" for d in dict.fromkeys(c["text_refs"]))
        add(f"| [{c['name']}]({c['url']}) | {c['version']} | {cell(c['license'] or 'UNKNOWN — could not be read')} "
            f"| {c['use']} | {c['platforms']} | {refs or '—'} |")
    add("")
    add("## JavaScript bundled into the desktop UI")
    add("")
    add("| Package | Version | License | Texts |")
    add("|---|---|---|---|")
    for c in js:
        refs = ", ".join(f"[{numbers[d]}](#text-{numbers[d]})" for d in dict.fromkeys(c["text_refs"]))
        add(f"| [{c['name']}]({c['url']}) | {c['version']} | {cell(c['license'] or 'UNKNOWN — could not be read')} | {refs or '—'} |")
    add("")
    add("Vite is a build tool and is not shipped, except for the small module-preload helper")
    add("it injects into the production bundle; that helper is covered by Vite's core license")
    add("reproduced below.")
    add("")
    add("## Windows installer")
    add("")
    add("The Windows installer is built by the Tauri bundler with NSIS. It contains the NSIS")
    add("installer stub — zlib/libpng license (the `Zlib` text below); NSIS's LZMA")
    add("compression module is under the Common Public License 1.0 with NSIS's special")
    add("exception, see <https://nsis.sourceforge.io/License> — and Tauri's")
    add("`nsis_tauri_utils` plugin from <https://github.com/tauri-apps/nsis-tauri-utils>")
    add("(Apache-2.0). Microsoft Edge WebView2 is not bundled: the installer downloads")
    add("Microsoft's bootstrapper when WebView2 is absent, under Microsoft's own terms.")
    add("")
    if mpl:
        add("## Components under the Mozilla Public License 2.0")
        add("")
        add("The files of these components are covered by MPL-2.0 and remain under MPL-2.0;")
        add("Switchboard's license does not apply to them. They are used unmodified. Their")
        add("source code form is available from the linked package page and the component's")
        add("own repository.")
        add("")
        for c in mpl:
            add(f"- [{c['name']} {c['version']}]({c['url']}) — `{c['license']}`")
        add("")
    if no_files:
        add("## Components that ship no license file")
        add("")
        add("These packages declare a license in their manifest but include no license file;")
        add("the canonical text of their declared license, under [License texts](#license-texts),")
        add("applies.")
        add("")
        for c in no_files:
            add(f"- {c['name']} {c['version']} — `{c['license'] or 'UNKNOWN'}`")
        add("")
    add("## License and notice files from the components")
    add("")
    add("Each distinct text is reproduced once, with the components that ship it. A full")
    add("Apache License 2.0 text with no filled-in copyright line is not repeated here; the")
    add("canonical Apache-2.0 text under [License texts](#license-texts) is that text.")
    add("")
    for digest, slot in texts.items():
        n = numbers[digest]
        add(f'<a id="text-{n}"></a>')
        add("")
        add(f"### Text {n}")
        add("")
        add("Used by: " + "; ".join(slot["users"]))
        add("")
        marker = fence(slot["body"])
        add(marker + "text")
        add(slot["body"])
        add(marker)
        add("")
    add("## License texts")
    add("")
    add(f"Canonical texts from {SPDX_DATA}, one per SPDX identifier used above.")
    add("")
    for identifier in ids:
        body = read_text(LICENSE_DIR / f"{identifier}.txt")
        add(f"### {identifier}")
        add("")
        marker = fence(body)
        add(marker + "text")
        add(body)
        add(marker)
        add("")
    return "\n".join(out).rstrip("\n") + "\n"


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.split("\n", 1)[0])
    parser.add_argument("--check", action="store_true", help="fail if the file is stale")
    args = parser.parse_args()
    try:
        content = build()
    except NoticeError as error:
        print(f"third-party notices: {error}", file=sys.stderr)
        return 2
    if args.check:
        current = OUTPUT.read_text(encoding='utf-8') if OUTPUT.exists() else ""
        if current != content:
            print(
                "THIRD_PARTY_NOTICES.md is stale; run `python3 scripts/third_party_notices.py`",
                file=sys.stderr,
            )
            return 1
        print(f"THIRD_PARTY_NOTICES.md is current ({len(content)} bytes)")
        return 0
    OUTPUT.write_text(content)
    print(f"wrote {OUTPUT.relative_to(ROOT)} ({len(content)} bytes)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
