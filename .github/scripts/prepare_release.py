#!/usr/bin/env python3
"""Bump the workspace version and roll CHANGELOG.md's Unreleased section.

Usage: prepare_release.py <version> [--date YYYY-MM-DD] [--repo owner/name]
Fails when the version is malformed, not newer than the current one, or the
Unreleased section is empty. Prints nothing but errors.
"""
import argparse
import datetime
import re
import sys
from pathlib import Path

SEMVER = re.compile(r"^(\d+)\.(\d+)\.(\d+)(-[0-9A-Za-z.-]+)?$")


def key(version):
    m = SEMVER.match(version)
    return tuple(int(x) for x in m.groups()[:3]) + (m.group(4) is None,)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("version")
    ap.add_argument("--date", default=datetime.date.today().isoformat())
    ap.add_argument("--repo", default="Niller2005/flupmix")
    ap.add_argument("--root", default=".")
    args = ap.parse_args()
    root = Path(args.root)
    new = args.version.removeprefix("v")
    if not SEMVER.match(new):
        sys.exit(f"error: '{new}' is not a semantic version like 1.2.3")

    cargo = root / "Cargo.toml"
    text = cargo.read_text()
    m = re.search(r'^version = "([^"]+)"', text, re.M)
    if not m:
        sys.exit("error: workspace version not found in Cargo.toml")
    old = m.group(1)
    if key(new) <= key(old):
        sys.exit(f"error: {new} is not newer than the current version {old}")
    cargo.write_text(text[: m.start(1)] + new + text[m.end(1) :], encoding="utf-8")

    log = root / "CHANGELOG.md"
    c = log.read_text()
    head = re.search(r"^## \[Unreleased\]\n", c, re.M)
    if not head:
        sys.exit("error: CHANGELOG.md has no '## [Unreleased]' section")
    nxt = re.search(r"^## \[", c[head.end() :], re.M)
    end = head.end() + nxt.start() if nxt else len(c)
    body = c[head.end() : end].strip("\n")
    refs = re.search(r"^\[Unreleased\]:.*$", c, re.M)
    if nxt is None and refs:
        end = min(end, refs.start())
        body = c[head.end() : end].strip("\n")
    if not body.strip():
        sys.exit("error: the Unreleased section of CHANGELOG.md is empty; describe the changes first")
    c = c[: head.end()] + f"\n## [{new}] - {args.date}\n\n{body}\n\n" + c[end:].lstrip("\n")

    base = f"https://github.com/{args.repo}"
    new_refs = (
        f"[Unreleased]: {base}/compare/v{new}...HEAD\n"
        f"[{new}]: {base}/compare/v{old}...v{new}\n"
    )
    if refs:
        c = re.sub(r"^\[Unreleased\]:.*\n", lambda _: new_refs, c, count=1, flags=re.M)
    else:
        c = c.rstrip("\n") + "\n\n" + new_refs
    log.write_text(c, encoding="utf-8")


if __name__ == "__main__":
    main()
