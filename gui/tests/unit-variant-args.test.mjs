// 提取真实的 run() 归一化逻辑验证：unit 变体必须发字符串，带字段的必须发 map
import fs from "fs";
import path from "path";
import { fileURLToPath } from "url";

// Resolve against this file rather than the working directory: the workflows
// run these tests from the repository root, while running them by hand from
// gui/ is just as reasonable.
const here = path.dirname(fileURLToPath(import.meta.url));
const html = fs.readFileSync(path.join(here, "..", "src", "index.html"), "utf8");

const start = html.indexOf("async function run(action");
const body = html.slice(start, start + 3000);
const unitLine = body.match(/const unit = typeof action === "string";/);
const payloadLine = body.match(/const payload = unit \? \{ \[action\]: null \} : action;/);
const invokeLine = body.match(/invoke\("run_action", \{ action: unit \? action : payload \}\)/);

let pass = 0, fail = 0;
const check = (n, got, want) => {
  const ok = JSON.stringify(got) === JSON.stringify(want);
  console.log(`${ok ? "ok  " : "FAIL"} ${n}`);
  ok ? pass++ : fail++;
};

check("run() 识别 unit 变体", !!unitLine, true);
check("run() 为 unit 变体造 payload", !!payloadLine, true);
check("invoke 对 unit 变体发字符串", !!invokeLine, true);

// 模拟：验证两种入参产出的 invoke 载荷
const build = (action) => {
  const unit = typeof action === "string";
  const payload = unit ? { [action]: null } : action;
  return unit ? action : payload;
};
check("string 'up' -> 'up'", build("up"), "up");
check("map {fetch:{interface}} -> 原 map", build({ fetch: { interface: "x" } }), { fetch: { interface: "x" } });

// 真实前端不应再有 unit 变体的空 map 调用
check("没有 run({up:{}}) 残留", /run\(\{\s*up:\s*\{\s*\}\s*\}\)/.test(html), false);

// The privileges screen used to offer "Authorise…" that only ran `up`, which
// changes nothing about read permission -- so the user authorised, watched a
// fetch happen, and landed back on the same screen. Guard the fix: the primary
// action must be the one that makes future reads work.
const grantBlock = html.slice(html.indexOf("const needsPriv ="), html.indexOf("function renderEmpty"));
check("权限界面主按钮是授予访问", /label: "Allow my account access"/.test(grantBlock), true);
check("授予访问调用 grant_access", /grantAccess\(\)/.test(html) && /invoke\("grant_access"\)/.test(html), true);
check("权限界面不再用 up 冒充授权", /onClick: \(\) => run\("up"\)/.test(grantBlock), false);

console.log(`\nPASS=${pass} FAIL=${fail}`);
process.exit(fail ? 1 : 0);

