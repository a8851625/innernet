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
    ${extract("serviceChecklistItem")}
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
  const o = render(snap({
    denied: true,
    service: { manager: "launchd", installed: true, running: false, path: "/x.plist" },
  }));
  check(o.title === "One-time setup", "titled as a one-time setup");
  check(o.body.includes("one authorisation"), "promises a single authorisation");
  check(
    o.actions.filter((a) => a.kind === "primary").length === 1,
    "exactly one primary action, not a menu of them"
  );
  check(
    o.actions.some((a) => a.label === "Grant access"),
    "the primary action grants standing access"
  );
}

// --- the boot service is surfaced, not buried in settings -----------------
{
  const stopped = render(snap({
    denied: true,
    service: { manager: "launchd", installed: true, running: false, path: "/x.plist" },
  }));
  const row = stopped.checklist.find((c) => c.label === "Runs at boot");
  check(!!row, "boot service appears on the setup screen");
  check(row.state === "todo", "a stopped service reads as outstanding");
  check(row.note === "installed but stopped", "and says which case it is");

  const missing = render(snap({ denied: true, service: { manager: "launchd", installed: false, running: false, path: "/x.plist" } }));
  check(
    missing.checklist.find((c) => c.label === "Runs at boot").note === "not installed",
    "an absent service is distinguishable from a stopped one"
  );

  const running = render(snap({ denied: true, service: { manager: "launchd", installed: true, running: true, path: "/x.plist" } }));
  check(
    running.checklist.find((c) => c.label === "Runs at boot").state === "ok",
    "a running service is not reported as outstanding"
  );
}

// --- an old CLI that reports no service state must not break the screen ---
{
  const o = render(snap({ denied: true, service: null }));
  check(o.checklist.length > 0, "screen still renders without service data");
  check(
    !o.checklist.some((c) => c.label === "Runs at boot"),
    "no service row is invented when the CLI reports none"
  );
}

console.log(`\nPASS=${pass} FAIL=${fail}`);
process.exit(fail === 0 ? 0 : 1);
