#!/usr/bin/env python3
"""Smoke-test the TUI file manager in a pty with a real window size."""
import os, pty, sys, time, fcntl, termios, struct, select, signal
import pyte

BINARY = os.path.abspath("./target/debug/my-tui-fm")

def run(keys, rows=30, cols=110, settle=0.35, cwd=None, binary=None):
    binary = binary or BINARY
    screen = pyte.Screen(cols, rows)
    stream = pyte.ByteStream(screen)
    pid, fd = pty.fork()
    if pid == 0:
        os.environ["TERM"] = "xterm-256color"
        if cwd:
            os.chdir(cwd)
        try:
            os.execv(binary, ["my-tui-fm"])
        finally:
            os._exit(1)
    fcntl.ioctl(fd, termios.TIOCSWINSZ, struct.pack("HHHH", rows, cols, 0, 0))
    raw = bytearray()
    snaps = []
    def pump(dur):
        end = time.time() + dur
        while time.time() < end:
            r, _, _ = select.select([fd], [], [], 0.05)
            if r:
                try:
                    data = os.read(fd, 65536)
                except OSError:
                    return
                if not data:
                    return
                raw.extend(data)
                stream.feed(data)
    for k in keys:
        if isinstance(k, float):
            pump(k)
        else:
            os.write(fd, k)
            pump(settle)
        # Snapshot the screen after every step (popups open and close, so
        # checks need per-step states, not just the final one).
        snaps.append("\n".join("".join(row).rstrip() for row in screen.display))
    pump(0.5)
    try:
        os.kill(pid, signal.SIGKILL)
    except ProcessLookupError:
        pass
    os.waitpid(pid, 0)
    os.close(fd)
    # final screen text, one line per row
    text = "\n".join("".join(row).rstrip() for row in screen.display)
    return bytes(raw), snaps

def has_screen(text, needle):
    return needle in text

def has(raw, needle):
    if isinstance(needle, str):
        needle = needle.encode("utf-8")
    return needle in raw

GIT_REPO = "/tmp/fm_git_repo"

def ensure_git_repo():
    """Create a small git repo with modifications for the git/rename tests."""
    import subprocess
    if os.path.isdir(GIT_REPO):
        return
    os.makedirs(f"{GIT_REPO}/src", exist_ok=True)
    def git(*args, **kw):
        subprocess.run(["git", *args], cwd=GIT_REPO, capture_output=True,
                       **{k: v for k, v in kw.items()})
    git("init", "-q", "-b", "main")
    with open(f"{GIT_REPO}/a.txt", "w") as f:
        f.write("hello\n")
    git("add", "a.txt")
    git("-c", "user.email=t@t", "-c", "user.name=t", "commit", "-qm", "init")
    with open(f"{GIT_REPO}/a.txt", "a") as f:
        f.write("world\n")
    with open(f"{GIT_REPO}/new_file.txt", "w") as f:
        f.write("new\n")

PREVIEW_DIR = "/tmp/fm_previews"

def ensure_preview_files():
    """Create a test ZIP for the archive preview smoke checks."""
    import zipfile
    os.makedirs(PREVIEW_DIR, exist_ok=True)
    zpath = f"{PREVIEW_DIR}/test_archive.zip"
    if not os.path.exists(zpath):
        with zipfile.ZipFile(zpath, "w") as z:
            z.writestr("a.txt", "hello\n")
            z.writestr("sub/b.txt", "world\n")

def entry_index(name, files=None):
    """Index of `name` in the app's sorted listing (dirs first, then
    case-insensitive by name — mirrors Explorer::sort_entries)."""
    files = files or sorted(os.listdir(PREVIEW_DIR))
    is_dir = os.path.isdir(os.path.join(PREVIEW_DIR, name))
    key = lambda n: (not os.path.isdir(os.path.join(PREVIEW_DIR, n)), n.lower())
    return sorted(files, key=key).index(name)

CHECKS = []
def check(name, ok):
    CHECKS.append((name, ok))
    print(f"{'OK ' if ok else 'MISS'} {name}")

def main(which):
    if which in ("all", "basic"):
        raw, s = run([1.2, b"j", 0.5, b"j", 0.5, b"q", 0.6])
        t = s[0]
        check("basic: app renders", "my-tui-fm" in t and "NORMAL" in t and "Ready" in t)

    if which in ("all", "help"):
        raw, s = run([1.2, b"?"] + [b"j", 0.1] * 30 + [0.4, b"\x1b", 0.4, b"q", 0.6])
        opened = s[1]
        scrolled = s[60]  # after the 30th j — clamped to the bottom
        check("help: popup opens", "Help" in opened and "NORMAL MODE" in opened and "fuzzy find" in opened)
        check("help: scrolls to all sections", "COMMAND MODE" in scrolled and "VISUAL MODE" in scrolled)

    if which in ("all", "fuzzy"):
        raw, s = run([1.2, b"f", 1.0, b"e", 0.2, b"x", 0.2, b"p", 0.8, b"\x1b", 0.4, b"q", 0.6])
        opened = s[2]
        typed = s[7]  # after typing 'exp'
        check("fuzzy: popup opens", "Find file" in opened)
        check("fuzzy: explorer.rs matches 'exp'", "explorer.rs" in typed)
        check("fuzzy: query line", "/ exp" in typed)

    if which in ("all", "git") or which in ("all", "rename") or which in ("all", "history"):
        ensure_git_repo()

    if which in ("all", "git"):
        raw, s = run([1.8, 0.2, b"q", 0.6], cwd="/tmp/fm_git_repo")
        t = s[0]
        check("git: branch badge + changed count",
              "main" in t and ("1 changed" in t or "2 changed" in t))
        check("git: M glyph on modified", "M " in t)
        check("git: + glyph on untracked", "+ " in t)
        raw2, s2 = run([1.5, 0.2, b"q", 0.6])
        check("git: no badge outside a repo", "changed" not in s2[0])

    if which in ("all", "du"):
        raw, s = run([1.2, b"S", 1.4, b"q", 0.6])
        t = s[1]
        check("du: badge + recursive sizes", "du " in t or "du …" in t)

    if which in ("all", "search"):
        raw, s = run([1.2, b"/", 0.4, b"r", 0.3, b"s", 0.7, b"\x1b", 0.4, b"q", 0.6])
        t = s[3]
        check("search: mode + match count", "SEARCH" in t and "match(es)" in t)

    if which in ("all", "rename"):
        # one j moves past the sorted-first directory (src/) to a.txt
        raw, s = run([1.2, b"j", 0.3, b"r", 0.9, b"\x1b", 0.4, b"q", 0.6],
                     cwd="/tmp/fm_git_repo")
        t = s[4]  # after 'r' + settle
        check("rename: current name prefilled", "rename a.txt" in t)

    if which in ("all", "history"):
        # cd into a subdir, Ctrl+o back, Alt+→ forward
        raw, s = run([1.2, b":", 0.3, b"c", 0.15, b"d", 0.15, b" ", 0.15, b"s", 0.15, b"r", 0.15, b"c", 0.6,
                      b"\r", 0.6, b"\x0f", 0.7, b"\x1b[1;3C", 0.7, b"q", 0.6], cwd="/tmp/fm_git_repo")
        after_cd = s[16]   # after Enter + settle
        after_back = s[18] # after Ctrl+o + settle
        after_fwd = s[20]  # after Alt+→ + settle
        check("history: cd into src (empty dir)", "directory is empty" in after_cd)
        check("history: Ctrl+o back to root", "a.txt" in after_back)
        check("history: Alt+\u2192 forward to src", "src" in after_fwd)

    if which in ("all", "visual"):
        raw, s = run([1.2, b"v", 0.4, b"j", 0.4, b"j", 0.5, b"\x1b", 0.4, b"q", 0.6], cwd="/tmp/fm_git_repo")
        t = s[5]  # after two j's: 3 items selected
        check("visual: multi-select count", "VISUAL · 3" in t)

    if which in ("all", "sidebar"):
        ensure_preview_files()
        # Tab focuses the sidebar; j moves; Enter jumps
        raw, s = run([1.5, b"\t", 0.7, b"j", 0.3, b"j", 0.3, b"\r", 0.8, b"q", 0.6], rows=30, cols=140, cwd=PREVIEW_DIR)
        opened = s[1]
        check("sidebar: places/disks/bookmarks render",
              "Places" in opened and "Disks" in opened and "Bookmarks" in opened and "downloads" in opened)
        check("sidebar: disks show free space", "free" in opened)

    if which in ("all", "extract"):
        ensure_preview_files()
        import shutil
        # clean any previous extraction
        shutil.rmtree(os.path.join(PREVIEW_DIR, "test_archive"), ignore_errors=True)
        idx = entry_index("test_archive.zip")
        keys = [1.5] + [b"j", 0.15] * idx + [0.3, b"X", 1.5, b"q", 0.6]
        raw, s = run(keys, rows=30, cols=160, cwd=PREVIEW_DIR)
        t = s[2 * idx + 2]
        check("extract: X extracts the archive",
              "Extracted" in t and os.path.isdir(os.path.join(PREVIEW_DIR, "test_archive")))

    if which in ("all", "compress"):
        ensure_preview_files()
        import os as _os
        target = _os.path.join(PREVIEW_DIR, "smoke.tar.gz")
        if _os.path.exists(target):
            _os.remove(target)
        idx = entry_index("test_archive.zip")
        keys = [1.5] + [b"j", 0.15] * idx + [0.3, b":", 0.3] + [bytes([c]) for c in b"targz smoke"] + [0.6, b"\r", 1.5, b"q", 0.6]
        raw, s = run(keys, rows=30, cols=160, cwd=PREVIEW_DIR)
        t = s[2 * idx + 17]  # after Enter + settle (renders lag keystrokes)
        check("compress: :targz creates an archive",
              "Created smoke.tar.gz" in t and _os.path.exists(target))

    if which in ("all", "cmdhist"):
        raw, s = run([1.2, b":", 0.3, b"t", 0.15, b"h", 0.15, b"e", 0.15, b"m", 0.15, b"e", 0.6, b"\r", 0.5,
                      b":", 0.4, b"\x1b[A", 0.5, b"q", 0.6])
        t = s[18]  # after ↑ + settle
        check("cmd history: ↑ recalls theme", ":theme" in t)

    print(f"\n{sum(1 for _, ok in CHECKS if ok)}/{len(CHECKS)} checks passed")

if __name__ == "__main__":
    main(sys.argv[1] if len(sys.argv) > 1 else "all")
