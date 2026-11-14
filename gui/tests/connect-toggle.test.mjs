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
  const m = html.match(/const connected = ([^;\r\n]+);/);
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

// --- the header state must not be derived from `is_up` ----------------------
//
// `is_up` in the snapshot means "the WireGuard device could be opened", which
// is a permissions answer and stays true after the service stops. Deriving the
// header from it showed "connected" for a tunnel that was not running, and the
// peer rows underneath made it look corroborated.
{
  const m = html.match(/const running = ([^;\r\n]+);/);
  if (!m) throw new Error("`running` not found in index.html");
  // eslint-disable-next-line no-new-func
  const isRunning = new Function("ifaces", `return ${m[1]};`);

  check(
    isRunning([{ is_up: true, service: { running: false } }]) === false,
    "is_up alone must not read as connected"
  );
  check(
    isRunning([{ is_up: false, service: { running: true } }]) === true,
    "the service decides, not the device flag"
  );
  check(
    isRunning([{ is_up: true, service: { running: true } }]) === true,
    "both true is connected"
  );
  check(isRunning([]) === false, "no interfaces is not connected");
}

// --- the peer count must be online peers, not the roster --------------------
{
  const m = html.match(/const onlinePeers = ([\s\S]*?)\.length;/);
  if (!m) throw new Error("`onlinePeers` not found in index.html");
  // eslint-disable-next-line no-new-func
  const countOnline = new Function("ifaces", `return (${m[1]}).length;`);

  // The reported shape: four configured members, two of them reachable.
  const roster = [
    {
      peers: [
        { name: "innernet-server", is_connected: true, is_you: false },
        { name: "home-mac", is_connected: false, is_you: false },
        { name: "mac-office", is_connected: false, is_you: false },
        { name: "node47", is_connected: true, is_you: false },
      ],
    },
  ];
  check(countOnline(roster) === 2, "counts only peers that are actually connected");

  // This device is listed too; it must never be counted as a peer.
  const withSelf = [
    {
      peers: [
        { name: "me", is_connected: true, is_you: true },
        { name: "other", is_connected: true, is_you: false },
      ],
    },
  ];
  check(countOnline(withSelf) === 1, "excludes this device");

  check(countOnline([]) === 0, "no interfaces counts zero");
}

// --- a stopped network must not render the peer list at all ---------------
//
// Showing the rows with a caption was not enough: handshake times and transfer
// counters still read as current state, so a stopped tunnel looked alive. When
// the service is not running the card has to stop early and render only the
// disconnected panel.
{
  // The branch that does it, taken from the shipped file.
  const branch = html.match(/if \(!connected\) \{\s*net\.appendChild\(renderDisconnectedPanel\(iface\)\);\s*return net;\s*\}/);
  check(!!branch, "a stopped network returns the disconnected panel before any tabs");

  // The panel must offer the action that fixes it.
  const panelStart = html.indexOf("function renderDisconnectedPanel(");
  if (panelStart < 0) throw new Error("renderDisconnectedPanel not found");
  const panelEnd = html.indexOf("\nfunction ", panelStart + 1);
  const panel = html.slice(panelStart, panelEnd < 0 ? html.length : panelEnd);

  check(
    /install_service/.test(panel),
    "the disconnected panel's Connect installs the service"
  );
  check(
    !/renderPeer\(/.test(panel) && !/handshake/.test(panel),
    "the disconnected panel renders no peers and no handshake times"
  );
  check(
    /openNetworkSettings/.test(panel),
    "Settings stays reachable while disconnected"
  );
}

// --- no manual sync control ------------------------------------------------
//
// `innernet fetch` needs root, and when the service is stopped it refuses
// outright ("Interface is not up"). The button's only possible outcomes were a
// password prompt followed by an error, or a duplicate of what the daemon
// already does every interval. Nothing it could do was useful.
{
  check(!/Sync now/.test(html), "no manual sync button");

  // Nothing may invoke `fetch` from the UI. It previously backed a "Reveal in
  // Finder" button that never revealed anything -- it prompted for a password
  // and changed network state instead.
  check(!/run\(\{\s*fetch\s*:/.test(html), "the UI never invokes fetch");
  // Matches a button label, not a comment explaining why it was removed.
  check(
    !/label: \s*"Reveal in Finder"/.test(html),
    "the mislabelled Reveal button is gone"
  );
}

console.log(`PASS=${pass} FAIL=${fail}`);
process.exit(fail > 0 ? 1 : 0);
