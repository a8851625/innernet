// Locks the first-run screen's contract.
//
// The screen exists because an empty interface list on macOS is ambiguous:
// "you never joined a network" and "your network is configured but the
// WireGuard sockets are root-only" look identical to the app. Telling the user
// the former when the latter is true sends them hunting for an invitation they
// do not need, so the two states must never be conflated.
//
// The functions under test are extracted from the shipped index.html rather
// than copied, so the test cannot drift from the real implementation.

import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const here = path.dirname(fileURLToPath(import.meta.url));
const html = fs.readFileSync(path.join(here, "..", "src", "index.html"), "utf8");

function extract(name) {
  const start = html.indexOf("function " + name + "(");
  if (start < 0) throw new Error("function not found in index.html: " + name);
  let depth = 0;
  for (let i = html.indexOf("{", start); i < html.length; i++) {
    if (html[i] === "{") depth++;
    else if (html[i] === "}") {
      depth--;
      if (depth === 0) return html.slice(start, i + 1);
    }
  }
  throw new Error("unbalanced braces in " + name);
}

function loadScene() {
  const sandbox = `
    const esc = (s) => String(s == null ? "" : s);
    const setStatus = () => {};
    const load = () => {};
    const openJoinDialog = () => {};
    function renderState(o) { globalThis.__rendered = o; }
    ${extract("renderSetup")}
    globalThis.__renderSetup = renderSetup;
  `;
  // eslint-disable-next-line no-new-func
  new Function(sandbox)();
}

function render(snapshot) {
  globalThis.__rendered = null;
  globalThis.__renderSetup(snapshot);
  return globalThis.__rendered;
}

let pass = 0;
let fail = 0;
function check(cond, what) {
  if (cond) pass++;
  else {
    console.log("FAIL: " + what);
    fail++;
  }
}

const snap = ({ denied, service }) => ({
  binary: "/Applications/innernet.app/Contents/MacOS/innernet",
  config_dir: "/etc/innernet",
  permission_denied: denied,
  interfaces: service === null ? [] : [{ name: "heheda-mesh", service }],
});

loadScene();

// --- the screen offers exactly one authorisation, not a prompt per click ----
{
  const o = render(snap({ denied: true, service: null }));
  check(o.title === "One-time setup", "titled as a one-time setup");
  check(
    o.body.includes("One authorisation"),
    "promises a single authorisation"
  );
  const primary = o.actions.filter((a) => a.kind === "primary");
  check(primary.length === 1, "exactly one primary action");
  check(
    primary[0].label === "Finish setup",
    "the primary action finishes the install rather than asking for access alone"
  );
}

// --- setup is not a menu of install steps -----------------------------------
{
  const o = render(snap({ denied: true, service: null }));
  const labels = o.actions.map((a) => a.label.toLowerCase()).join(" ");
  // The app used to leave `install-service` to the user, which meant "installed"
  // and "usable" were different states. Finishing setup must cover it.
  check(
    !labels.includes("install service") && !labels.includes("start service"),
    "does not ask the user to install the boot service by hand"
  );
  const checks = o.checklist.map((c) => c.label.toLowerCase()).join(" ");
  check(
    checks.includes("boot"),
    "says the network will be kept connected at boot"
  );
}

// --- the checklist reflects that connecting is part of setup ----------------
{
  const o = render(snap({ denied: true, service: null }));
  const boot = o.checklist.find((c) => c.label.toLowerCase().includes("boot"));
  check(!!boot, "has a boot row");
  check(boot.state === "todo", "boot row is pending until setup runs");
}


// --- setup appears whenever the service is missing ---------------------------
//
// The bug this locks: setup used to key off "the interface list is empty". Once
// the daemon started publishing a snapshot the list was never empty, so the
// screen never appeared and the service was never installed -- leaving an app
// that displayed data from a daemon that did not exist.
//
// The rule is read out of the shipped index.html rather than restated here. An
// earlier version of this test re-implemented the predicate, so when the real
// one was `!(i.service && i.service.installed)` -- true for every interface
// whenever the CLI omitted `service`, pinning users to the setup screen -- the
// test still passed. A test that copies the code it checks cannot catch a bug
// in that code.
function setupRule() {
  const m = html.match(/const needsSetup = ([^;\r\n]+);/);
  if (!m) throw new Error("needsSetup not found in index.html");
  // eslint-disable-next-line no-new-func
  return new Function("ifaces", `return ${m[1]};`);
}

{
  const needsSetupFor = setupRule();

  check(
    needsSetupFor([
      {
        name: "heheda-mesh",
        service: { manager: "launchd", installed: false, running: false, path: "/x.plist" },
      },
    ]),
    "an uninstalled service requires setup even with peers visible"
  );

  check(
    !needsSetupFor([
      {
        name: "heheda-mesh",
        service: { manager: "launchd", installed: true, running: true, path: "/x.plist" },
      },
    ]),
    "an installed service does not require setup"
  );

  // The regression that shipped: CLI 2.5.1 has no `service` key at all. The app
  // cannot tell "not installed" from "this CLI does not say", and choosing the
  // former traps a working machine on the setup screen forever.
  check(
    !needsSetupFor([{ name: "heheda-mesh" }]),
    "a CLI that omits `service` must not be read as 'not installed'"
  );
}

// --- a disconnected network is not a setup problem --------------------------
//
// Pressing Disconnect removes the boot service, so `installed` becomes false.
// The old branch treated that as "needs setup" and showed the authorisation
// screen -- asking for an administrator password to fix something that only
// needs the Connect button, for a network the app could already read.
{
  const m = html.match(/const unreadable = ([^;\r\n]+);/);
  if (!m) throw new Error("`unreadable` not found in index.html");
  // eslint-disable-next-line no-new-func
  const isUnreadable = new Function("snapshot", `return ${m[1]};`);

  check(
    isUnreadable({ permission_denied: true, interfaces: [] }) === true,
    "a permission-denied snapshot is unreadable"
  );
  check(
    isUnreadable({ permission_denied: false, interfaces: [{ name: "heheda-mesh" }] }) === false,
    "a snapshot the app could read is not unreadable"
  );
  check(isUnreadable(null) === false, "no snapshot is not the same as denied");

  // The gate itself, read out of the shipped file. Copying it here would pass
  // whatever the test said, not whatever the app does -- which is how the
  // previous version of this block stayed green while the app showed the
  // authorisation screen after Disconnect.
  //
  // The condition contains nested parentheses, so it is extracted by balancing
  // them rather than with a regex that stops at the first `)`.
  const marker = "else if (needsSetup";
  const at = html.indexOf(marker);
  if (at < 0) throw new Error("the needsSetup branch not found in index.html");

  const open = html.indexOf("(", at + "else if ".length);
  let depth = 0;
  let close = -1;
  for (let i = open; i < html.length; i++) {
    if (html[i] === "(") depth++;
    else if (html[i] === ")") {
      depth--;
      if (depth === 0) {
        close = i;
        break;
      }
    }
  }
  if (close < 0) throw new Error("unbalanced parentheses in the needsSetup branch");
  // Everything after `needsSetup` inside the parens.
  const condition = html.slice(open + 1, close).replace(/^needsSetup/, "").trim();

  const ifaces = [
    {
      name: "heheda-mesh",
      service: { manager: "launchd", installed: false, running: false, path: "/x.plist" },
    },
  ];
  const needsSetup = ifaces.some((i) => i.service && !i.service.installed);
  const unreadable = isUnreadable({ permission_denied: false, interfaces: ifaces });

  check(needsSetup, "removing the service does make needsSetup true");

  // Evaluate the real condition with the values a disconnected-but-readable
  // network produces.
  // eslint-disable-next-line no-new-func
  const showsSetup = new Function(
    "needsSetup",
    "unreadable",
    "ifaces",
    `return !!(needsSetup ${condition});`
  );
  check(
    !showsSetup(needsSetup, unreadable, ifaces),
    "a readable, non-empty network must NOT be sent to the setup screen"
  );

  // And the case setup is actually for: nothing readable at all.
  check(showsSetup(true, true, []), "an unreadable machine with no interfaces does show setup");
}

let failed = fail > 0;
console.log(`PASS=${pass} FAIL=${fail}`);
process.exit(failed ? 1 : 0);
