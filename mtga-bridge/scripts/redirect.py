#!/usr/bin/env python3
"""Toggle the MTGA -> local-backend hosts redirect.

Adds (or removes) a managed block in the OS hosts file that points MTG Arena's
bootstrap hostnames at a local/LAN address, so the real client connects to our
stub backend (mtga-bridge) instead of Wizards' servers. Autodetects the OS and
the correct hosts-file path.

  python3 redirect.py on            # redirect MTGA -> 127.0.0.1
  python3 redirect.py on -t 192.168.1.50   # redirect to another machine on the LAN
  python3 redirect.py off           # restore: talk to Wizards again
  python3 redirect.py status        # show whether the redirect is active

Only the hostnames in HOSTNAMES below are touched, and only inside the marker
block, so toggling is non-destructive to the rest of your hosts file.

Requires admin/root to edit the hosts file:
  Linux/macOS:  sudo python3 redirect.py on
  Windows:      run from an *Administrator* terminal:  python redirect.py on

NOTE: which hostnames must be redirected is still being pinned down (the login /
FrontDoor flow is under active reverse-engineering). Edit HOSTNAMES as we learn
more -- the script needs no other changes.
"""

import argparse
import os
import platform
import shutil
import socket
import sys

# --- Hostnames MTGA dials during boot/login. Redirecting these sends the client
#     to our backend. The per-match GRE endpoint does NOT belong here: the client
#     learns it as *data* from the (stubbed) FrontDoor response, so we point that
#     by value, not via DNS. ---
HOSTNAMES = [
    "api.platform.wizards.com",       # PlayFab / platform auth broker (login)
    "api.platform-ref.wizards.com",   # platform ref/staging variant
    # FrontDoor + assets/telemetry hosts get appended here as we confirm them.
]

MARKER_BEGIN = "# >>> mtga-bridge redirect (managed) >>>"
MARKER_END = "# <<< mtga-bridge redirect (managed) <<<"
DEFAULT_TARGET = "127.0.0.1"


def hosts_path() -> str:
    """Return the OS hosts-file path."""
    if platform.system() == "Windows":
        root = os.environ.get("SystemRoot", r"C:\Windows")
        return os.path.join(root, "System32", "drivers", "etc", "hosts")
    return "/etc/hosts"  # Linux and macOS


def read_hosts(path: str) -> str:
    with open(path, "r", encoding="utf-8", errors="surrogateescape") as f:
        return f.read()


def strip_block(text: str) -> str:
    """Remove any existing managed block (idempotent)."""
    lines = text.splitlines(keepends=True)
    out, skipping = [], False
    for line in lines:
        stripped = line.strip()
        if stripped == MARKER_BEGIN:
            skipping = True
            continue
        if stripped == MARKER_END:
            skipping = False
            continue
        if not skipping:
            out.append(line)
    return "".join(out)


def build_block(target: str) -> str:
    width = max((len(h) for h in HOSTNAMES), default=0)
    body = "".join(f"{target}\t{h.ljust(width)}\n" for h in HOSTNAMES)
    return f"{MARKER_BEGIN}\n{body}{MARKER_END}\n"


def is_active(text: str) -> bool:
    return MARKER_BEGIN in text


def write_hosts(path: str, text: str) -> None:
    """Write atomically-ish with a one-time backup."""
    backup = path + ".mtga-bridge.bak"
    if not os.path.exists(backup):
        try:
            shutil.copy2(path, backup)
        except OSError:
            pass  # best effort; not fatal
    tmp = path + ".mtga-bridge.tmp"
    with open(tmp, "w", encoding="utf-8", errors="surrogateescape", newline="") as f:
        f.write(text)
    os.replace(tmp, path)


def flush_dns_hint() -> str:
    sysname = platform.system()
    if sysname == "Windows":
        return "ipconfig /flushdns"
    if sysname == "Darwin":
        return "sudo dscacheutil -flushcache; sudo killall -HUP mDNSResponder"
    return "sudo resolvectl flush-caches  (or: sudo systemd-resolve --flush-caches)"


def cmd_status(path: str) -> int:
    try:
        text = read_hosts(path)
    except OSError as e:
        print(f"cannot read {path}: {e}", file=sys.stderr)
        return 1
    active = is_active(text)
    print(f"hosts file : {path}")
    print(f"redirect   : {'ACTIVE -> MTGA points at our backend' if active else 'inactive -> MTGA talks to Wizards'}")
    if active:
        for line in text.splitlines():
            s = line.strip()
            if s and not s.startswith("#") and any(h in s for h in HOSTNAMES):
                print(f"  {s}")
    else:
        print("  managed hostnames:")
        for h in HOSTNAMES:
            try:
                print(f"    {h} -> {socket.gethostbyname(h)}")
            except OSError:
                print(f"    {h} -> (unresolved)")
    return 0


def cmd_set(path: str, target: str, enable: bool) -> int:
    try:
        text = read_hosts(path)
    except OSError as e:
        print(f"cannot read {path}: {e}", file=sys.stderr)
        return 1

    new_text = strip_block(text)
    if enable:
        if new_text and not new_text.endswith("\n"):
            new_text += "\n"
        new_text += build_block(target)

    if new_text == text:
        print("already in the requested state; nothing to do.")
        return 0

    try:
        write_hosts(path, new_text)
    except PermissionError:
        elev = ("an Administrator terminal" if platform.system() == "Windows"
                else "sudo")
        print(f"permission denied writing {path}.", file=sys.stderr)
        print(f"re-run with {elev}, e.g.:", file=sys.stderr)
        if platform.system() == "Windows":
            print(f"  python {os.path.basename(__file__)} {'on' if enable else 'off'}", file=sys.stderr)
        else:
            print(f"  sudo {sys.executable} {os.path.abspath(__file__)} {'on' if enable else 'off'}", file=sys.stderr)
        return 13
    except OSError as e:
        print(f"failed to write {path}: {e}", file=sys.stderr)
        return 1

    if enable:
        print(f"redirect ACTIVE: {len(HOSTNAMES)} hostname(s) -> {target}")
        for h in HOSTNAMES:
            print(f"  {h}")
    else:
        print("redirect removed: MTGA will talk to Wizards again.")
    print(f"\ntip: flush the DNS cache so the change takes effect:\n  {flush_dns_hint()}")
    return 0


def main() -> int:
    p = argparse.ArgumentParser(
        description="Toggle the MTGA -> local-backend hosts redirect.")
    p.add_argument("action", choices=["on", "off", "status"],
                   help="on = redirect to local backend; off = restore Wizards; status = show state")
    p.add_argument("-t", "--target", default=DEFAULT_TARGET,
                   help=f"address to redirect to (default {DEFAULT_TARGET}; use a LAN IP if the backend runs on another machine)")
    p.add_argument("--hosts-file", default=None,
                   help="override the hosts-file path (mainly for testing)")
    args = p.parse_args()

    path = args.hosts_file or hosts_path()

    if args.action == "status":
        return cmd_status(path)
    return cmd_set(path, args.target, enable=(args.action == "on"))


if __name__ == "__main__":
    raise SystemExit(main())
