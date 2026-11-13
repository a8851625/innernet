// Locks the network card's single-control contract.
//
// The card used to offer "Bring up" / "Bring down", and separately, inside
// Settings, "Install" / "Start now" / "Remove" for the boot service. Those are
// the same thing described twice: `innernet up` runs once and exits, taking the
// tun device with it, so the only way a network stays connected is the service.
// Two sets of controls meant a user could press "Bring up" and watch nothing
// happen, which is exactly what happened.
//
// One control, whose state is the machine's actual state.

import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const here = path.dirname(fileURLToPath(import.meta.url));
const html = fs.readFileSync(path.join(here, "..", "src", "index.html"), "utf8");

let pass = 0;
let fail = 0;
function check(cond, what) {
  if (cond) pass++;
  else {
    console.log("FAIL: " + what);
    fail++;
  }
}

check(!/Bring up/.test(html), "no 'Bring up' control");
check(!/Bring down/.test(html), "no 'Bring down' control");
check(!/Start now/.test(html), "no separate 'Start now' control");
check(!/svcInstall|svcStart|svcRemove/.test(html), "no leftover service buttons");

check(/toggle\.textContent = connected \? "Disconnect" : "Connect"/.test(html),
  "the card has one switch, labelled Connect or Disconnect");

check(/const connected = !!\(svc && svc\.running\)/.test(html),
  "the switch reflects the boot service, not the transient interface state");

// Connect must install the service: anything less leaves the tunnel down after
// the process exits, which is the bug this replaces.
check(/run\(\{ install_service: \{ interface: iface\.name \} \}\)/.test(html),
  "Connect installs the boot service");

check(/run\(\{ uninstall_service: \{ interface: iface\.name \} \}\)/.test(html),
  "Disconnect stops the boot service");

// Disconnecting must not be destructive: the config stays so reconnecting works.
check(/configuration is kept/i.test(html), "Disconnect says the configuration is kept");

// The checks above only prove the source *says* the right words. This one runs
// the expression, so a change that keeps the text but inverts the meaning (or
// reads the wrong field) fails here.
{
  const m = html.match(/const connected = (.*?);\n/);
  if (!m) throw new Error("`connected` not found in index.html");
  // eslint-disable-next-line no-new-func
  const isConnected = new Function("svc", `return ${m[1]};`);

  check(
    isConnected({ manager: "launchd", installed: true, running: true, path: "/x.plist" }) === true,
    "a running service reads as connected"
  );

  // The reported bug: the service was removed, but the snapshot on disk still
  // said running:true. Once the app trusts live service state instead, this
  // case must render as Connect.
  check(
    isConnected({ manager: "launchd", installed: false, running: false, path: "/x.plist" }) === false,
    "a removed service reads as disconnected"
  );

  check(
    isConnected({ manager: "launchd", installed: true, running: false, path: "/x.plist" }) === false,
    "an installed-but-stopped service reads as disconnected"
  );

  check(isConnected(undefined) === false, "a missing service reads as disconnected");
  check(isConnected(null) === false, "a null service reads as disconnected");
}

console.log(`PASS=${pass} FAIL=${fail}`);
process.exit(fail > 0 ? 1 : 0);
