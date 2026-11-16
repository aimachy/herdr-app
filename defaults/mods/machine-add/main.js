// Add a machine: the hosts in ~/.ssh/config to pick from (or type one),
// each added with one click (`hc machine add`). A machine with no config
// entry goes in through the password form instead: it answers in the
// dialog (password, a first host key to trust), never in a terminal.
// Config hosts that need an answer still get the terminal instructions.
// Machines that need no answers are added quietly. Opened as a dialog
// (`mod.open machine-add`); the sidebars' Add machine and the welcome
// open it.

import { View, div, svg } from "gpui-kit";
import { v_flex, h_flex, InputState, Input, TextView } from "gpui-base";
import { hc, exec, home, readTextFile, theme, props, closeOverlay } from "herdr";

const I = (name) => `mods/workspace-chrome/${name}.svg`;
const shellArg = (target) => /^[a-z0-9_@%+=:,./-]+$/i.test(target) ? target : "'" + target.replace(/'/g, "'\\''") + "'";

// An Include's files, as ssh finds them: ~ is home, a relative path is
// under ~/.ssh, and a pattern matches files (through sh; none where there's no sh).
async function includeFiles(value) {
  const paths = (value.match(/"[^"]*"|\S+/g) ?? [])
    .map((p) => p.replace(/^"|"$/g, ""))
    .map((p) => (p.startsWith("~/") ? `${home()}/${p.slice(2)}` : p.startsWith("/") || /^[a-z]:[\\/]/i.test(p) ? p : `${home()}/.ssh/${p}`));
  const files = [];
  for (const p of paths) {
    if (!/[*?[]/.test(p)) {
      files.push(p);
      continue;
    }
    // From a Windows program, Git's sh gets the pattern already expanded, one
    // argument per file, unless a space made it quoted: then sh expands it, and
    // takes a backslash as an escape. So read every argument, never split one
    // at its spaces, and give sh slashes.
    const glob = /^[a-z]:/i.test(p) ? p.replace(/\\/g, "/") : p;
    const r = await exec("sh", ["-c", 'IFS=; for p in "$@"; do for f in $p; do [ -f "$f" ] && printf "%s\\n" "$f"; done; done', "sh", glob]).catch(() => null);
    files.push(...(r?.stdout ?? "").split("\n").filter(Boolean));
  }
  return files;
}

// Host entries people can pick: named ones, not patterns, from the file
// and the files it includes (ssh stops 16 deep).
async function parseConfig(text, hosts = [], depth = 0) {
  let cur = null;
  for (const raw of (text ?? "").split("\n")) {
    const m = raw.trim().match(/^([^\s#=]+)\s*=?\s*(.*)$/);
    if (!m) continue;
    const [key, value] = [m[1].toLowerCase(), m[2].trim()];
    if (key === "host" || key === "match") {
      const names = key === "host" ? value.split(/\s+/).filter((h) => h && !/[*?!]/.test(h) && !hosts.some((x) => x.alias === h || x.aliases.includes(h))) : [];
      cur = names.length ? { alias: names[0], aliases: names.slice(1), hostname: null, user: null } : null;
      if (cur) hosts.push(cur);
    } else if (key === "include" && depth < 16) {
      for (const f of await includeFiles(value)) await parseConfig(await readTextFile(f).catch(() => null), hosts, depth + 1);
    } else if (cur && key === "hostname") cur.hostname = value;
    else if (cur && key === "user") cur.user = value;
  }
  return hosts;
}

export default class MachineAdd extends View {
  init(_props, cx) {
    this.hosts = null;
    this.added = [];
    this.state = {}; // target -> "adding" | error text
    this.pending = new Set();
    this.refreshing = false;
    this.typed = "";
    this.input = InputState.new({ placeholder: "user@host" });
    this.input.on("change", (_e, cx) => { this.typed = this.input.value(); cx.notify(); });
    this.input.on("submit", (_e, cx) => this.add(this.typed, cx));
    this.inputFocused = true;
    this.input.on("focus", (_e, cx) => { this.inputFocused = true; cx.notify(); });
    this.input.on("blur", (_e, cx) => { this.inputFocused = false; cx.notify(); });
    this.input.focus();
    cx.timer.after(600, () => this.input.focus());
    // The password form: host/user/password/port, for a machine with no
    // ssh config entry and no key yet. The password lives only in this
    // dialog's memory: it goes to hc over stdin once, to plant a key.
    this.form = { host: "", user: "", pass: "", port: "" };
    this.formState = null;
    this.formAdding = false;
    this.trust = null; // a presented host key's fingerprint, awaiting the person's yes
    this.passwordDisabled = null; // { host, user, command }: that server takes no password logins at all
    this.formFocus = null;
    this.formInputs = {};
    for (const [key, placeholder] of [["host", "IP or host name"], ["port", "Port (22)"], ["user", "User name"], ["pass", "Password"]]) {
      const input = InputState.new({ placeholder });
      input.on("change", (_e, cx) => { this.form[key] = input.value(); cx.notify(); });
      input.on("focus", (_e, cx) => { this.formFocus = key; cx.notify(); });
      input.on("blur", (_e, cx) => { if (this.formFocus === key) this.formFocus = null; cx.notify(); });
      if (key === "pass") input.on("submit", (_e, cx) => this.addWithPassword(cx));
      this.formInputs[key] = input;
    }
    this.macs = {}; // target -> { name, core } for an added Mac
    this.opened = {};
    this.handle = cx.focus_handle();
    cx.spawn(async (cx) => {
      this.hosts = await parseConfig(await readTextFile(`${home()}/.ssh/config`).catch(() => null));
      await this.refresh();
      cx.notify();
    });
    cx.timer.every(3000, (cx) => {
      if ((!this.pending.size && !this.added.length) || this.refreshing) return;
      this.refreshing = true;
      cx.spawn(async (cx) => {
        try {
          await this.refresh();
          cx.notify();
        } finally {
          this.refreshing = false;
        }
      });
    });
  }

  async refresh() {
    const r = await hc(["machine", "ls", "--json"]).catch(() => null);
    try {
      this.added = r?.code === 0 ? (JSON.parse(r.stdout).machines ?? []).filter((m) => !m.temporary) : [];
    } catch {
      this.added = [];
    }
    for (const target of this.pending) {
      if (!this.isAdded(target)) continue;
      this.pending.delete(target);
      delete this.state[target];
      if (this.typed.trim() === target) {
        this.input.set_value("");
        this.typed = "";
      }
      await this.checkMac(target);
    }
  }

  isAdded(target) {
    return this.added.some((m) => m.target === target || m.name === target || m.host === target);
  }

  add(target, cx) {
    target = target.trim();
    if (!target || this.state[target] === "adding") return;
    this.pending.delete(target);
    this.state[target] = "adding";
    cx.notify();
    cx.spawn(async (cx) => {
      const r = await hc(["machine", "add", target, "--json"]).catch((e) => ({ code: 1, stderr: String(e), stdout: "" }));
      let why = null;
      try {
        why = r.code === 0 ? null : (JSON.parse(r.stdout || r.stderr).error ?? { message: (r.stderr || r.stdout).trim() });
      } catch {
        why = { message: (r.stderr || r.stdout || "it didn't answer").trim() };
      }
      if (!why) {
        delete this.state[target];
        this.input.set_value("");
        this.typed = "";
        await this.refresh();
        await this.checkMac(target);
      } else if (["hc_missing", "incompatible", "login_needed", "auth_failed", "host_key"].includes(why.kind) || /password|passphrase|host key|permission denied|login/i.test(why.message ?? "")) {
        const quoted = shellArg(target);
        this.pending.add(target);
        this.state[target] = `Couldn't add ${target} here: it needs you to answer something (a password, a new host key, or installing herdr-core there). Run \`hc machine add ${quoted}\` in a terminal. \`hc machine --help\` shows the options.`;
      } else this.state[target] = why.message || "it didn't work";
      await this.refresh();
      cx.notify();
    });
  }

  // Add by password: hc's embedded ssh logs in once, plants a per-machine
  // key, and pins the host key — after the person trusts its fingerprint.
  // Errors arrive as kinds and each gets its own words, here in the dialog.
  addWithPassword(cx, trustKey = null) {
    const host = this.form.host.trim(), user = this.form.user.trim(), port = this.form.port.trim() || "22";
    if (!host || !user || this.formAdding) return;
    if (!this.form.pass) { this.formState = "Type the password for that machine."; cx.notify(); return; }
    this.formAdding = true;
    this.formState = null;
    this.passwordDisabled = null;
    if (!trustKey) this.trust = null;
    cx.notify();
    cx.spawn(async (cx) => {
      const args = ["machine", "add", "--host", host, "--user", user, "--port", port, "--password-stdin", "--json"];
      if (trustKey) args.push("--trust-host-key", trustKey);
      const r = await hc(args, { stdin: this.form.pass }).catch((e) => ({ code: 1, stderr: String(e), stdout: "" }));
      this.formAdding = false;
      let why = null;
      try {
        why = r.code === 0 ? null : (JSON.parse(r.stdout || r.stderr).error ?? { message: (r.stderr || r.stdout).trim() });
      } catch {
        why = { message: (r.stderr || r.stdout || "it didn't answer").trim() };
      }
      if (!why) {
        this.formState = null;
        this.trust = null;
        this.formInputs.pass.set_value("");
        this.form.pass = "";
        await this.refresh();
        // Added: the password's one job is done, so the dialog closes
        // with it. The config list above stays open on purpose — people
        // add several machines there in a row.
        this.close();
      } else if (why.kind === "host_key") {
        this.trust = (why.message ?? "").match(/SHA256:[A-Za-z0-9+/=]+/)?.[0] ?? null;
        this.formState = this.trust
          ? `First contact: ${host} presents the host key ${this.trust}. Check it against the server itself (its console, or your hoster's panel) and trust it only if it matches.`
          : why.message;
      } else if (why.kind === "password_auth_disabled") {
        // Not a wrong password: that server takes no password logins at
        // all, so the password was never checked. Keep this.trust — the
        // host key was presented before the password was tried, and the
        // retry after the fix still needs the person's yes on it. The
        // command rides the error's hint, alone on its last line.
        const lines = (why.hint ?? "").split("\n").map((l) => l.trim()).filter(Boolean);
        this.passwordDisabled = { host, user, command: lines[lines.length - 1] ?? "" };
        this.formState = null;
      } else if (why.kind === "auth_failed" || why.kind === "needs_login") {
        this.trust = null;
        this.formState = "Login failed: the machine refused that user name or password.";
      } else if (why.kind === "unreachable") {
        this.trust = null;
        this.formState = `Couldn't reach ${host}:${port}: check the address, and that ssh is running there.`;
      } else if (why.kind === "hc_missing") {
        this.trust = null;
        this.formState = "That machine has no herdr-core yet: run the fleet install script on it first (it installs herdr-core and Pi), then add it here.";
      } else {
        this.trust = null;
        this.formState = why.message || "it didn't work";
      }
      cx.notify();
    });
  }

  formField(key, flex) {
    const t = theme();
    const el = Input.new(this.formInputs[key])
      .h(34)
      .px(10)
      .gap(6)
      .items_center()
      .rounded(8)
      .border_1()
      .border_color(this.formFocus === key ? t.accent : t.input)
      .bg(t.background)
      .text_size(13)
      .text_color(t.foreground);
    return flex ? el.flex_1() : el.w(96);
  }

  // That server takes no password logins at all: say so plainly (it is not
  // a wrong password), give the one command that turns password login on
  // for just this account, and let the person retry here afterwards.
  passwordDisabledCard(t) {
    const d = this.passwordDisabled;
    const command = d.command;
    return v_flex().id("ma-pw-disabled").mt(10).p(12).gap(8).rounded(10)
      .bg(t.blocked + "14").border_1().border_color(t.blocked + "40")
      .child(div().text_size(12.8).font_semibold().text_color(t.foreground).child(`Password logins are off on ${d.host}`))
      .child(div().text_size(12.5).line_height("18px").text_color(t.muted)
        .child(`That server does not accept password logins for ${d.user}, so your password was never checked — this is not a wrong password. To turn password login on for just your account, run this on ${d.host} once (it needs sudo), then add again here:`))
      .when(Boolean(command), (el) => el.child(h_flex().gap(8).items_center()
        .child(div().flex_1().min_w(0).text_size(12).text_color(t.foreground).child(command))
        .child(div().id("ma-pw-disabled-copy").px(10).py(4).rounded(6).bg(t.raised).text_color(t.foreground)
          .text_size(12).cursor_pointer().hover(s => s.bg(t.hover))
          .on_click((_e, cx) => { cx.write_to_clipboard(command); this.copiedCommand = command; cx.notify(); })
          .child(this.copiedCommand === command ? "Copied" : "Copy"))));
  }

  formSection(t) {
    const ready = this.form.host.trim() && this.form.user.trim() && !this.formAdding;
    return v_flex()
      .child(div().pt(14).pb(6).text_size(11.5).font_medium().text_color(t.dim).child("Or add with a password"))
      .child(div().pb(8).text_size(12.5).line_height("18px").text_color(t.muted)
        .child("For a fresh VPS: the password signs in once, plants a key, and is never saved. From then on the key does the signing in."))
      .child(h_flex().gap(8).child(this.formField("host", true)).child(this.formField("port", false)))
      .child(div().h(8))
      .child(h_flex().gap(8).child(this.formField("user", true)).child(this.formField("pass", true)))
      .child(
        h_flex()
          .pt(10)
          .gap(8)
          .items_center()
          .child(
            div()
              .id("ma-form-add")
              .h(34)
              .px(14)
              .flex()
              .items_center()
              .rounded(8)
              .bg(t.solid)
              .text_color(t.solid_foreground)
              .text_size(12.5)
              .font_medium()
              .when(!ready, (el) => el.opacity(0.4))
              .cursor_pointer()
              .on_click((_e, cx) => this.addWithPassword(cx))
              .child(this.formAdding ? "Connecting…" : "Add"),
          )
          .when(Boolean(this.trust), (el) =>
            el.child(
              div()
                .id("ma-form-trust")
                .h(34)
                .px(14)
                .flex()
                .items_center()
                .rounded(8)
                .border_1()
                .border_color(t.blocked + "40")
                .bg(t.blocked + "14")
                .text_color(t.foreground)
                .text_size(12.5)
                .font_medium()
                .cursor_pointer()
                .hover((s) => s.opacity(0.88))
                .on_click((_e, cx) => this.addWithPassword(cx, this.trust))
                .child("Trust this key and add"),
            ),
          ),
      )
      .when(Boolean(this.formState), (el) => el.child(div().pt(8).text_size(12).line_height("17px").text_color(t.blocked).child(this.formState)))
      .when(Boolean(this.passwordDisabled), (el) => el.child(this.passwordDisabledCard(t)));
  }

  // macOS asks on its own screen before anything reads Desktop, Documents
  // or Downloads, and the asking program waits: say so once, with the way
  // to allow it for good.
  async checkMac(target) {
    const m = this.added.find((x) => x.target === target || x.name === target);
    if (!m) return;
    const r = await exec("sh", ["-c", 'uname; command -v herdr-core || echo "$HOME/.local/bin/herdr-core"'], { on: m.name }).catch(() => null);
    const [os, core] = (r?.stdout ?? "").trim().split("\n");
    if (os === "Darwin") this.macs[target] = { name: m.name, core: core ?? "~/.local/bin/herdr-core" };
  }

  logoutCard(t, m) {
    const command = `sudo loginctl enable-linger -- ${shellArg(m.logout_linger_user ?? "<user>")}`;
    return v_flex().id(`ma-logout-${m.name}`).mx(10).mb(8).p(12).gap(8).rounded(10)
      .bg(t.blocked + "14").border_1().border_color(t.blocked + "40")
      .child(div().text_size(12.8).font_semibold().text_color(t.foreground).child(`Agents on ${m.name} stop when you disconnect`))
      .child(div().text_size(12.5).line_height("18px").text_color(t.muted)
        .child("The server ends your programs when your last login closes, for example when your laptop sleeps."))
      .child(div().text_size(12.5).text_color(t.muted).child(`Run this on ${m.name} once:`))
      .child(h_flex().gap(8).items_center()
        .child(div().flex_1().min_w(0).text_size(12).text_color(t.foreground).child(command))
        .child(div().id(`ma-logout-copy-${m.name}`).px(10).py(4).rounded(6).bg(t.raised).text_color(t.foreground)
          .text_size(12).cursor_pointer().hover(s => s.bg(t.hover))
          .on_click((_e, cx) => { cx.write_to_clipboard(command); this.copiedCommand = command; cx.notify(); })
          .child(this.copiedCommand === command ? "Copied" : "Copy")));
  }

  macCard(t, target) {
    const mac = this.macs[target];
    return v_flex()
      .mx(10)
      .mb(8)
      .p(12)
      .gap(8)
      .rounded(10)
      .bg(t.blocked + "14")
      .border_1()
      .border_color(t.blocked + "40")
      .child(div().text_size(12.8).font_semibold().text_color(t.foreground).child(`${mac.name} is a Mac: allow its folders once`))
      .child(
        div()
          .text_size(12.5)
          .line_height("18px")
          .text_color(t.muted)
          .child("macOS asks on the Mac's own screen before an agent reads Desktop, Documents or Downloads, and waits until someone answers. Give herdr-core Full Disk Access there and it never asks again: in the list, press + and pick this file:"),
      )
      .child(div().px(8).py(4).rounded(6).bg(t.background).border_1().border_color(t.border).text_size(12).text_color(t.foreground).child(mac.core))
      .child(
        h_flex()
          .gap(8)
          .items_center()
          .child(
            div()
              .id(`ma-mac-${target}`)
              .h(28)
              .px(12)
              .flex()
              .items_center()
              .rounded(7)
              .bg(t.solid)
              .text_color(t.solid_foreground)
              .text_size(12)
              .font_medium()
              .cursor_pointer()
              .hover((s) => s.opacity(0.88))
              .on_click((_e, cx) => {
                exec("open", ["x-apple.systempreferences:com.apple.preference.security?Privacy_AllFiles"], { on: mac.name }).catch(() => null);
                this.opened[target] = true;
                cx.notify();
              })
              .child(`Open the setting on ${mac.name}`),
          )
          .when(Boolean(this.opened[target]), (el) => el.child(div().text_size(12).text_color(t.dim).child("It's open on the Mac's screen."))),
      );
  }

  close() {
    closeOverlay(props().overlay);
  }

  key(e, cx) {
    if (e.key === "escape") { this.close(); cx.stop_propagation(); }
  }

  message(t, target) {
    const text = String(this.state[target]).replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");
    const command = `hc machine add ${shellArg(target)}`;
    return v_flex().gap(5)
      .child(div().text_size(12).text_color(t.blocked).child(TextView.html(`ma-message-${target}`, `<span>${text}</span>`).selectable(true)))
      .when(this.pending.has(target), (el) => el.child(h_flex().child(div()
        .id(`ma-copy-${target}`).px(10).py(4).rounded(6).bg(t.raised).text_color(t.foreground)
        .text_size(12).cursor_pointer().hover(s => s.bg(t.hover))
        .on_click((_e, cx) => { cx.write_to_clipboard(command); this.copiedCommand = command; cx.notify(); })
        .child(this.copiedCommand === command ? "Copied" : "Copy"))));
  }

  row(t, target, label, sub) {
    const st = this.state[target];
    const done = this.isAdded(target);
    const machine = this.added.find(m => m.target === target || m.name === target || m.host === target);
    const right = done
      ? h_flex().gap(5).items_center().text_size(12).text_color(t.ansi2).child(svg(I("circle-check")).size(13)).child("Added")
      : st === "adding"
        ? h_flex().gap(6).items_center().text_size(12).text_color(t.muted).child(svg(I("spinner")).size(12).text_color(t.working).spin(900)).child("Connecting")
        : div()
              .id(`ma-add-${target}`)
              .px(10)
              .h(26)
              .flex()
              .items_center()
              .rounded(7)
              .border_1()
              .border_color(t.input)
              .bg(t.raised)
              .text_size(12)
              .font_medium()
              .cursor_pointer()
              .hover((s) => s.bg(t.hover))
              .on_click((_e, cx) => this.add(target, cx))
              .child("Add");
    const idle = !done && !st;
    return v_flex()
      .flex_none()
      .child(
        h_flex()
          .id(`ma-row-${target}`)
          .h(40)
          .px(10)
          .gap(10)
          .items_center()
          .rounded(8)
          .hover((s) => s.bg(t.hover))
          .when(idle, (el) => el.cursor_pointer().on_click((_e, cx) => this.add(target, cx)))
          .child(svg(I("server")).size(15).text_color(t.muted))
          .child(div().flex_none().text_size(13).text_color(t.foreground).child(label))
          .child(div().flex_1().min_w(0).truncate().text_size(12).text_color(t.dim).child(sub ?? ""))
          .child(right),
      )
      .when(typeof st === "string" && st !== "adding", (el) => el.child(div().px(35).pb(6).child(this.message(t, target))))
      .when(Boolean(this.macs[target]), (el) => el.child(this.macCard(t, target)))
      .when(machine?.keeps_running_after_logout === false, el => el.child(this.logoutCard(t, machine)));
  }

  render() {
    const t = theme();
    const hosts = this.hosts ?? [];
    return v_flex()
      .id("machine-add")
      .track_focus(this.handle)
      .on_key_down((e, cx) => this.key(e, cx))
      .w_full()
      .min_h(0)
      .p(18)
      .bg(t.popover)
      .text_color(t.foreground)
      .child(h_flex().items_center().child(div().flex_1().text_size(15).font_semibold().child("Add a machine")).child(div().id("ma-esc").px(6).py(2).rounded(5).text_size(11).text_color(t.dim).cursor_pointer().hover((s) => s.bg(t.hover).text_color(t.foreground)).on_click(() => this.close()).child("Esc")))
      .child(
        div()
          .pt(6)
          .text_size(12.8)
          .line_height("19px")
          .text_color(t.muted)
          .child("A VPS, a Mac mini, a box at home: Herdr reaches it over ssh, and agents there keep running while your laptop sleeps."),
      )
      .child(div().pt(16).pb(6).text_size(11.5).font_medium().text_color(t.dim).child(hosts.length ? "From your ssh config" : "Your ssh config lists no hosts"))
      .child(
        v_flex()
          .id("ma-hosts")
          .max_h(280)
          .min_h(0)
          .flex_shrink(1)
          .overflow_y_scroll()
          .children(
            this.hosts === null
              ? [div().px(10).py(8).text_size(12.5).text_color(t.dim).child("Reading ~/.ssh/config…")]
              : hosts.filter(h => !this.isAdded(h.alias)).map((h) => this.row(t, h.alias, h.alias, [h.user, h.hostname].filter(Boolean).join("@") || (h.aliases.length ? `also ${h.aliases.join(", ")}` : ""))),
          ),
      )
      .when(this.added.length > 0, el => el.child(v_flex().id("ma-added").max_h(280).min_h(0).overflow_y_scroll()
        .children(this.added.map(m => this.row(t, m.target, m.name, m.target)))))
      .child(div().pt(14).pb(6).text_size(11.5).font_medium().text_color(t.dim).child("Or type how ssh reaches it"))
      .child(
        h_flex()
          .gap(8)
          .child(
            Input.new(this.input)
              .flex_1()
              .h(34)
              .px(10)
              .gap(6)
              .items_center()
              .rounded(8)
              .border_1()
              .border_color(this.inputFocused ? t.accent : t.input)
              .bg(t.background)
              .text_size(13)
              .text_color(t.foreground),
          )
          .child(
            div()
              .id("ma-add-typed")
              .h(34)
              .px(14)
              .flex()
              .items_center()
              .rounded(8)
              .bg(t.solid)
              .text_color(t.solid_foreground)
              .text_size(12.5)
              .font_medium()
              .when(!this.typed.trim(), (el) => el.opacity(0.4))
              .cursor_pointer()
              .on_click((_e, cx) => this.add(this.typed, cx))
              .child("Add"),
          ),
      )
      .when(Boolean(this.typed.trim() && this.state[this.typed.trim()]), (el) => el.child(div().pt(6).child(this.message(t, this.typed.trim()))))
      .child(this.formSection(t));
  }
}
