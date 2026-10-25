/**
 * 通知端到端验证：创建即将截止的任务 → 等待调度器触发通知 →
 * 截屏取证 + 复测 IPC 存活。
 */
const j = await fetch("http://127.0.0.1:9223/json").then((r) => r.json());
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
    setTimeout(() => rej(new Error("timeout " + method)), 8000);
    ws.send(JSON.stringify({ id, method, params }));
  });
const ev = (expr) => send("Runtime.evaluate", { expression: expr, returnByValue: true, awaitPromise: true });
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

console.log("[1] create task due in 40s with at-deadline reminder");
const r = await ev(`(async () => {
  const inv = window.__TAURI_INTERNALS__.invoke;
  const t = await inv('create_task', { input: {
    title: '通知验证任务', description: '', category_id: null,
    priority: 'high', due_at: new Date(Date.now() + 40000).toISOString(), reminders: [0]
  }});
  return t.id;
})()`);
console.log("created task id:", r.result?.result?.value);

console.log("[2] wait 75s for scheduler to fire notification...");
await sleep(75000);

console.log("[3] IPC alive after notification?");
const t0 = Date.now();
try {
  const s = await ev("(async () => { const inv = window.__TAURI_INTERNALS__.invoke; const st = await inv('get_app_state'); return { tasks: st.tasks.length }; })()");
  console.log(`IPC OK in ${Date.now() - t0}ms, tasks:`, s.result?.result?.value?.tasks);
} catch (e) {
  console.log("IPC HUNG after notification:", e.message);
  process.exit(2);
}
process.exit(0);
