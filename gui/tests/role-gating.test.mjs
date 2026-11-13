// Verifies the role gating used by the UI: who may invite peers and manage
// the network, and which actions stay available to ordinary members.
//
// The functions are extracted from index.html rather than copied, so this test
// cannot drift from the shipped front-end.
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";

const here = dirname(fileURLToPath(import.meta.url));
const html = readFileSync(join(here, "..", "src", "index.html"), "utf8");

function extract(re, what) {
  const m = html.match(re);
  if (!m) throw new Error("could not find " + what + " in index.html");
  return m[0];
}

const src = [
  extract(/function roleOf\(iface\) \{[\s\S]*?\n\}/, "roleOf"),
  extract(/function isAdmin\(iface\) \{[\s\S]*?\n\}/, "isAdmin"),
  extract(/const ADMIN_ACTIONS = new Set\(\[[\s\S]*?\]\);/, "ADMIN_ACTIONS"),
].join("\n\n");

// Evaluate the extracted front-end code in this module's scope.
const factory = new Function(src + "\nreturn { roleOf, isAdmin, ADMIN_ACTIONS };");
const { roleOf, isAdmin, ADMIN_ACTIONS } = factory();

const mk = (opts = {}) => ({
  name: "net", is_up: true, address: "10.60.1.2", network_cidr_prefix: 16,
  peers: [
    { name: "me", is_you: true, is_admin: false, is_disabled: false, ...opts },
    { name: "other", is_you: false, is_admin: true, is_disabled: false },
  ],
});

let pass = 0, fail = 0;
const check = (label, got, want) => {
  const ok = got === want;
  console.log((ok ? "  ok   " : "  FAIL ") + label + " -> " + got + (ok ? "" : " (want " + want + ")"));
  ok ? pass++ : fail++;
};

check("自己 is_admin=false", roleOf(mk()), "member");
check("自己 is_admin=true", roleOf(mk({ is_admin: true })), "admin");
check("自己被 disabled", roleOf(mk({ is_disabled: true })), "disabled");
check("member isAdmin()", isAdmin(mk()), false);
check("admin isAdmin()", isAdmin(mk({ is_admin: true })), true);
check("无 peers", roleOf({ peers: [] }), "unknown");

// 门控集合本身
for (const a of ["add_peer","disable_peer","enable_peer","rename_peer","add_cidr",
                 "rename_cidr","delete_cidr","add_association","delete_association"]) {
  check("ADMIN_ACTIONS 含 " + a, ADMIN_ACTIONS.has(a), true);
}
for (const a of ["uninstall","down","up","fetch","set_listen_port","override_endpoint",
                 "unset_listen_port","unset_endpoint","override_peer_endpoint","unset_peer_endpoint",
                 // A boot service is this machine's own business: a plain member
                 // must be able to keep their interface up across reboots
                 // without asking a network admin.
                 "install_service","uninstall_service"]) {
  check("member 可执行 " + a, ADMIN_ACTIONS.has(a), false);
}
console.log("\nPASS=" + pass + " FAIL=" + fail);
process.exit(fail ? 1 : 0);
