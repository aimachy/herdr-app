# Installing the aimachy Herdr fork

This fork of Herdr is built around one flow: point the desktop app at a
fresh Linux server with nothing but its address, a user, a port and a
password — no `~/.ssh/config` editing, no key setup first — and land in
a space where Pi is already running.

How it differs from upstream Herdr:

- **Add a machine with a password.** The add-machine dialog takes a
  host, a port, a user and a password. The password signs in once, over
  an SSH client embedded in herdr-core; the host key is shown first and
  pinned only when you trust it; a key made for that one machine is
  planted on the server and used from then on. The password itself is
  never saved and never passed as a command-line argument.
- **A server that takes no password logins says so.** Instead of a
  sign-in that simply fails, you get a card explaining that password
  authentication is off on that server (your password was never
  checked), with the one command that turns it on for just your
  account.
- **Self-update points at this fork.** The app and herdr-core update
  from this fork's GitHub Releases, never from upstream's, so a fork
  install never silently becomes an upstream one.
- **A one-shot server installer.** `fleet/install.sh` (in the
  herdr-core repo) installs herdr-core and Pi on the server and
  pre-creates a space named after the server, with Pi already running
  in it.

> **Status, November 2026.** Both halves have fork releases:
> herdr-core v0.1.22 and the desktop app v0.1.16. §3 starts with
> downloading the app; building it from source remains as an
> alternative. The macOS app build is ad-hoc signed and the Windows
> build is unsigned (§6), so expect the usual first-run warnings from
> those systems.

## 1. What you need

- A desktop machine: macOS (Apple Silicon), Linux (x86-64 or ARM64,
  with a desktop session), or Windows (x64) — the same platforms as
  upstream.
- A Linux server (x86-64 or ARM64) reachable over SSH, and an account
  on it that you can sign in to with a password. The password is only
  needed for the first sign-in. If the server has password logins
  switched off (many cloud images do), §5 shows the one command that
  turns them on for your account only.
- On the server: `curl` or `wget`, `python3`, and Node.js/npm (for
  the Pi step; skippable).

## 2. Server setup

Run this **on the server**, as the user who should own the setup.
Everything lands under that user's `$HOME`; nothing needs sudo.

Fetch the two installer files (they must sit side by side) and run:

```sh
mkdir -p ~/fleet && cd ~/fleet
curl -fsSLO https://raw.githubusercontent.com/aimachy/herdr-core/fleet/fleet/install.sh
curl -fsSLO https://raw.githubusercontent.com/aimachy/herdr-core/fleet/fleet/precreate.py
sh install.sh
```

(Cloning the herdr-core repo and running `sh fleet/install.sh` from
the `fleet` branch works too.)

The script does four things:

1. **Installs herdr-core** to `~/.local/bin/herdr-core` (plus an `hc`
   symlink). By default it downloads the build for your platform from
   this fork's Releases manifest and verifies its sha256 before
   unpacking. Offline alternatives: `--core-bin <file>` installs a
   binary you copied over yourself; `--core-url <url>` downloads from
   your own URL (add `--core-sha256 <hex>`).
2. **Aligns the core with login sessions (Linux).** It tries to enable
   systemd linger for your user and makes sure the core listens where
   SSH login sessions look for it, so an install driven from a
   non-login context (cloud-init, cron, `sudo -u`) stays reachable.
   If it cannot enable linger, it prints the exact
   `sudo loginctl enable-linger <user>` command instead of failing
   silently.
3. **Installs Pi** — the npm package `@mariozechner/pi-coding-agent`.
   If `pi` is already on your `PATH` the step is skipped; if `npm` is
   missing the script stops with a clear error (install Node.js, or
   re-run with `--skip-pi`).
4. **Pre-creates a space** named after the server's hostname
   (`--space-name` overrides it), holding one terminal named `pi`
   running `pi`, so the desktop app shows a named space with a live Pi
   the moment it connects.

Two promises, by design:

- **The installer never writes an API key** — not into Pi's config,
  not anywhere — and never touches any `.env` file. Configuring Pi's
  provider, model and credentials is yours to do afterwards (Pi keeps
  its configuration under `~/.pi/agent/` on the server).
- **Re-running is safe.** Existing pieces are skipped; if the
  pre-created space's terminal has ended, the space is repaired in
  place; a failed space creation rolls back what it made.

Full option list and internals: see `fleet/README.md` in the
herdr-core repo.

### Just the core, nothing else

If you only want herdr-core on a machine (no Pi, no pre-created
space), the release's own installer is one line:

```sh
curl -fsSL https://github.com/aimachy/herdr-core/releases/latest/download/install.sh | sh
```

On Windows:

```powershell
irm https://github.com/aimachy/herdr-core/releases/latest/download/install.ps1 | iex
```

## 3. Desktop app

### Install: download a release

Download the package for your platform from the fork's Releases —
<https://github.com/aimachy/herdr-app/releases/latest> (the first
release is v0.1.16):

- **Windows (x64):** `herdr-app-windows-x86_64.tar.gz` — extract it
  and run `herdr-app\herdr-app.exe` from the extracted folder. The
  build is unsigned, so SmartScreen will warn on first run.
- **macOS (Apple Silicon):** `herdr-app-macos-aarch64.tar.gz` —
  extract it and move `Herdr.app` to Applications. The build is
  ad-hoc signed, not notarized, so Gatekeeper will refuse a plain
  double-click: right-click the app and choose **Open** the first
  time.
- **Linux (x86-64 or ARM64):** `herdr-app-linux-x86_64.tar.gz` (or
  `herdr-app-linux-aarch64.tar.gz`) — extract it and run
  `herdr-app/herdr-app` from the extracted folder.

Notes:

- Do not use the installers on `releases.herdr.dev`: they install the
  upstream app, not this fork.
- The app needs a herdr-core on this computer too; it fetches it from
  this fork's Releases, so it matches the fork. From then on the app
  updates itself from this fork's Releases (§4).

### Alternative: build from source

Build the app from the `fleet` branch of this repo. You need git,
Rust 1.95.0 and Zig 0.16.0 (the versions CI uses), and
[just](https://github.com/casey/just); Linux also needs the
development packages listed in `.github/workflows/ci.yml`.

```sh
git clone --recurse-submodules https://github.com/aimachy/herdr-app.git
cd herdr-app
git checkout fleet
just build
just install
```

The fork's changes live on the `fleet` branch. The default branch is
a plain mirror of upstream and does **not** have them. A build from
source does not update itself (§4).

### Add the server

1. In the app, open **Add machine** (from the machines list in the
   sidebar, or Settings → Machines).
2. In the **Or add with a password** section, fill in the host (IP or
   name), the port (22 if unsure), your user on the server, and its
   password, then click **Add**.
3. On first contact the dialog shows the server's host key
   fingerprint (`SHA256:…`). Check it against the server itself — its
   console, or your hoster's panel — and click **Trust this key and
   add** only if it matches. The fingerprint is then pinned in the
   machine's record; a different key later is refused (see §5).
4. The password signs in once and a per-machine key is planted;
   the machine appears in the sidebar. Open the space named after
   the server: Pi is already running in it. From now on the key does
   the signing in — the password is not needed again and was never
   saved (it is held only in the dialog's memory and cleared on
   success).

If the dialog instead says **"That machine has no herdr-core yet: run
the fleet install script on it first"**, the server side is missing —
go back to §2, then add again.

Machines already present in your `~/.ssh/config` can still be added
the classic way from the same dialog.

### Adding from the command line instead

The same flow exists in `hc` (useful on a machine without the app):

```sh
hc machine add --host 203.0.113.10 --user ubuntu --port 22 --password-stdin
# type the password on its own line, then Enter
```

The first attempt stops with a `host_key` error that prints the
fingerprint. After checking it, repeat with the fingerprint pinned:

```sh
hc machine add --host 203.0.113.10 --user ubuntu --port 22 --password-stdin \
  --trust-host-key SHA256:…
```

## 4. Updating

- **herdr-core on this computer:** `hc update` downloads the latest
  release from this fork's Releases, verifies it, swaps the binary
  and restarts the core. Running terminals keep running.
- **herdr-core on an added machine:** `hc machine update <name>`
  copies this `hc` there and restarts the core there; its terminals
  keep running. Re-running `fleet/install.sh` on the server also
  updates the core in place.
- **The app:** a release build updates itself from
  `aimachy/herdr-app` Releases (it checks `app-latest.json`, verifies
  the download's sha256, and swaps itself). A build from source does
  not update itself — pull the `fleet` branch and rebuild.

## 5. Troubleshooting

### "Password logins are off on \<host\>"

The server does not accept password logins at all, so your password
was never checked — this is not a wrong password. Many cloud images
ship with password authentication off. Run this once **on the
server** (it needs sudo), then click **Add** again in the dialog:

```sh
printf 'Match User <user>\n    PasswordAuthentication yes\n' | sudo tee /etc/ssh/sshd_config.d/99-fleet-<user>.conf >/dev/null && (sudo systemctl reload ssh 2>/dev/null || sudo systemctl reload sshd)
```

Replace `<user>` with your user name on that server (the dialog's
card shows the command with your user already filled in, with a Copy
button). It enables password login for just that account; every
other account stays as it was. The dialog keeps the host-key trust
from the first attempt, so the retry goes straight through.

The same command also helps if the dialog only says the login was
refused: a server that offers keyboard-interactive but not the plain
password method cannot be added with the form yet (§6), and enabling
the password method with this command makes it work.

### "…presents host key …, not the pinned one"

The server's host key is not the one pinned when the machine was
added. That is expected if the server was rebuilt or reinstalled —
and it is also what a machine-in-the-middle would look like, so check
before proceeding. If the server really was rebuilt: remove the
machine (`hc machine rm <name>`, or in the app) and add it again,
which pins the new key after you check its fingerprint.

### "Login failed: the machine refused that user name or password"

The user name or password was refused. Check both and try again.
One look-alike: if the server offers **only** keyboard-interactive
authentication (no plain password method), the form cannot sign in
yet and the failure reads exactly like a refused password — see the
remedy in the first item above.

### "Couldn't reach \<host\>:\<port\>"

Check the address and port, that sshd is running on the server, and
that any firewall or cloud security group lets the port through.

### Installed on the server, but the app cannot connect

The core listens where the context that started it put it; an install
driven from a non-login context can leave it listening beside its
state, where SSH login sessions do not look. `fleet/install.sh`
already handles this (linger plus socket placement, §2 step 2) — if
you installed another way, run on the server:

```sh
sudo loginctl enable-linger <user>
```

then re-run `fleet/install.sh`. It moves a misplaced core only while
it serves no live terminals (a move would end them); with live
terminals it leaves the core in place and prints how to converge by
hand. Related: if the app shows a card **"Agents on \<machine\> stop
when you disconnect"**, that is the same linger setting — run the
command the card shows.

## 6. Known limitations

- **App builds are unsigned.** The macOS app is ad-hoc signed and
  not notarized, and the Windows app is unsigned: expect Gatekeeper
  and SmartScreen first-run warnings (§3). The macOS app build comes
  from CI and has not been run on a real Mac yet.
- **Windows servers** are not covered by the embedded password
  sign-in, which targets Unix sshd. Add Windows machines the classic
  way (system ssh / `~/.ssh/config`), as upstream.
- **Keyboard-interactive-only servers** cannot be added with the
  password form yet: the embedded SSH client answers the plain
  password method, not keyboard-interactive challenges. Enabling the
  password method (§5) works around it.
- **Release builds are unsigned.** The macOS core archives are
  cross-built, unsigned and not notarized: Gatekeeper will refuse the
  `Herdr Core.app` bundle; the standalone binary inside the archive
  is the usable path. The Windows core build is a MinGW cross-build
  (not upstream's MSVC build), unsigned, and needs Windows 10 or
  newer. The macOS builds have not been run on a real Mac yet.
- **The form's password field is a plain text field** — what you type
  is visible while you type it (no masked input yet). The password is
  held only in the dialog's memory and cleared on success; it is
  never saved, logged, or passed as an argument.
- **Pi's provider setup is manual.** The installer installs Pi's body
  only, deliberately without any API key or model configuration.
