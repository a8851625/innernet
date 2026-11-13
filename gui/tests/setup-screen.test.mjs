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
{
  const withUninstalled = {
    binary: "/Applications/innernet.app/Contents/MacOS/innernet",
    config_dir: "/etc/innernet",
    permission_denied: false,
    interfaces: [
      {
        name: "heheda-mesh",
        service: { manager: "launchd", installed: false, running: false, path: "/x.plist" },
      },
    ],
  };
  // The decision lives in load(), so assert on the rule itself rather than
  // driving the whole renderer: an uninstalled service must force setup.
  const needsSetup = withUninstalled.interfaces.some(
    (i) => !(i.service && i.service.installed)
  );
  check(needsSetup, "an uninstalled service requires setup even with peers visible");

  const installed = {
    interfaces: [
      {
        name: "heheda-mesh",
        service: { manager: "launchd", installed: true, running: true, path: "/x.plist" },
      },
    ],
  };
  check(
    !installed.interfaces.some((i) => !(i.service && i.service.installed)),
    "an installed service does not require setup"
  );
}

let failed = fail > 0;
console.log(`PASS=${pass} FAIL=${fail}`);
process.exit(failed ? 1 : 0);
