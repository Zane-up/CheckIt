/**
 * 通知横幅捕捉：CDP 创建 40s 后截止（截止时提醒）的任务，
 * 等待调度器触发（5s tick），在 t+42/46/50s 连拍截图。
 */
const j = await fetch("http://127.0.0.1:9223/json").then((r) => r.json()).catch(() => null);
if (!j) { console.log("CDP_NOT_AVAILABLE"); process.exit(1); }
const w = j.find((t) => t.title === "CheckIt" && t.url.includes("tauri.localhost"));
if (!w) { console.log("NO_WIDGET"); process.exit(1); }
const ws = await new Promise((res, rej) => {
  const x = new WebSocket(w.webSocketDebuggerUrl);
  x.addEventListener("open", () => res(x));
  x.addEventListener("error", () => rej(new Error("ws")));
});
let n = 1;
const pending = new Map();
ws.addEventListener("message", (ev) => {
  const m = JSON.parse(ev.data);
  if (m.id && pending.has(m.id)) { pending.get(m.id)(m); pending.delete(m.id); }
});
const send = (method, params = {}) =>
  new Promise((res, rej) => {
    const id = n++;
    pending.set(id, res);
    setTimeout(() => rej(new Error("timeout " + method)), 10000);
    ws.send(JSON.stringify({ id, method, params }));
  });
const ev = (expr) => send("Runtime.evaluate", { expression: expr, returnByValue: true, awaitPromise: true });
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

const t0 = Date.now();
const r = await ev(`(async () => {
  const inv = window.__TAURI_INTERNALS__.invoke;
  const t = await inv('create_task', { input: {
    title: 'ToastCapture', description: '', category_id: null,
    priority: 'high', due_at: new Date(Date.now() + 40000).toISOString(), reminders: [0]
  }});
  return t.id;
})()`);
console.log("created task", r.result?.result?.value, "at t=0");

// 等到接近触发时刻（40s 截止 + 最多 5s 调度延迟）
await sleep(41000);
console.log("t+41s: shooting x3");
console.log("SHOT1"); // 外层 shell 在此输出后截图
// 打印标记后自身等待，让外层脚本按标记截图
await sleep(1000);
console.log("SHOT2");
await sleep(1000);
console.log("SHOT3");
process.exit(0);
