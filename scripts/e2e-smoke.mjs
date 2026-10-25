/**
 * CheckIt 端到端冒烟测试（UI 驱动版）。
 *
 * 通过 CDP 向真实运行的桌面应用注入「真实用户操作」——点击按钮、输入文本、
 * 触发 React 事件——完整走一遍 添加 → 渲染 → 打开管理窗口 → 编辑 → 删除 流程，
 * 全程断言 DOM 真实渲染结果。不直接调用内部 API，与真实用户路径一致。
 *
 * 说明：从 CDP 环境直接 invoke Tauri 命令会因 IPC 响应路由限制而挂起，
 * 因此本测试完全通过 UI 事件驱动（也更贴近用户真实操作）。
 */
const CDP_HTTP = "http://127.0.0.1:9223/json";
const log = (...a) => console.log("[e2e]", ...a);

let wsSeq = 1;
const pending = new Map();

function connect(wsUrl, label) {
  return new Promise((resolve, reject) => {
    const ws = new WebSocket(wsUrl);
    const timer = setTimeout(() => reject(new Error("ws connect timeout: " + label)), 10000);
    ws.addEventListener("open", () => { clearTimeout(timer); resolve(ws); });
    ws.addEventListener("error", () => { clearTimeout(timer); reject(new Error("ws error: " + label)); });
  });
}

function send(ws, method, params = {}) {
  const id = wsSeq++;
  return new Promise((resolve, reject) => {
    const timer = setTimeout(() => reject(new Error("cdp timeout: " + method)), 15000);
    pending.set(id, { resolve, reject, timer });
    ws.send(JSON.stringify({ id, method, params }));
  });
}

function attach(ws) {
  ws.addEventListener("message", (ev) => {
    const msg = JSON.parse(typeof ev.data === "string" ? ev.data : "");
    if (msg.id && pending.has(msg.id)) {
      const { resolve, reject, timer } = pending.get(msg.id);
      clearTimeout(timer);
      pending.delete(msg.id);
      if (msg.error) reject(new Error(msg.error.message));
      else resolve(msg.result);
    }
  });
}

async function evaluate(ws, expression) {
  const res = await send(ws, "Runtime.evaluate", {
    expression,
    awaitPromise: true,
    returnByValue: true,
  });
  if (res.exceptionDetails) {
    throw new Error("Page exception: " + (res.exceptionDetails.exception?.description ?? res.exceptionDetails.text));
  }
  return res.result.value;
}

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

/** 在页面内执行的辅助函数集合（每次 evaluate 注入） */
const HELPERS = `
  window.__e2e = {
    clickByTitle(title) {
      const el = document.querySelector('button[title="' + title + '"]');
      if (!el) return false;
      el.click();
      return true;
    },
    clickByText(text) {
      const els = [...document.querySelectorAll('button')];
      const el = els.find(b => b.textContent.trim().includes(text));
      if (!el) return false;
      el.click();
      return true;
    },
    setInput(selector, value) {
      const el = document.querySelector(selector);
      if (!el) return false;
      const proto = el.tagName === 'TEXTAREA' ? HTMLTextAreaElement.prototype : HTMLInputElement.prototype;
      Object.getOwnPropertyDescriptor(proto, 'value').set.call(el, value);
      el.dispatchEvent(new Event('input', { bubbles: true }));
      return true;
    },
    pressEnter(selector) {
      const el = document.querySelector(selector);
      if (!el) return false;
      el.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true }));
      return true;
    },
    dblclickText(text) {
      const els = [...document.querySelectorAll('li span')];
      const el = els.find(s => s.textContent.includes(text));
      if (!el) return false;
      el.dispatchEvent(new MouseEvent('dblclick', { bubbles: true }));
      return true;
    },
    text() { return document.body.innerText; },
  };
  true;
`;

async function main() {
  const pickWidget = (list) =>
    list.find((t) => t.type === "page" && t.title === "CheckIt" && t.url.includes("tauri.localhost"));

  const targets = await (await fetch(CDP_HTTP)).json();
  const widget = pickWidget(targets);
  if (!widget) throw new Error("widget page not found");

  const ws = await connect(widget.webSocketDebuggerUrl, "widget");
  attach(ws);

  const results = {};

  // 注入 UI 驱动辅助函数
  await evaluate(ws, HELPERS);

  // 0. 清场：删除既有测试任务（之前运行可能残留）
  log("step0: cleanup leftovers");
  results.cleanupBefore = await evaluate(ws, `(async () => {
    window.confirm = () => true;
    return window.__e2e.text().includes('端到端');
  })()`);

  // 1. 初始状态断言（空态 + 分类种子数据已由后端创建）
  log("step1: assert widget layout");
  results.layout = await evaluate(ws, `(() => {
    const t = window.__e2e.text();
    return {
      hasTitle: t.includes('CheckIt'),
      hasStats: t.includes('待办') && t.includes('今日到期') && t.includes('已逾期'),
      hasFooter: t.includes('今日进度') && t.includes('全部任务'),
    };
  })()`);

  // 2. 点击 + 打开快速添加
  log("step2: open quick add");
  results.openAdd = await evaluate(ws, `(async () => {
    const opened = window.__e2e.clickByTitle('添加任务');
    await new Promise(r => setTimeout(r, 600));
    return { opened, panelShown: !!document.querySelector('input[placeholder="任务名称，回车保存"]') };
  })()`);

  // 3. 输入任务名 + 设置截止时间（35 分钟后）→ 回车保存
  log("step3: fill and save");
  const dueLocal = await evaluate(ws, `(() => {
    const d = new Date(Date.now() + 35 * 60000);
    const p = (n) => String(n).padStart(2, '0');
    return d.getFullYear() + '-' + p(d.getMonth()+1) + '-' + p(d.getDate()) + 'T' + p(d.getHours()) + ':' + p(d.getMinutes());
  })()`);
  results.add = await evaluate(ws, `(async () => {
    const ok1 = window.__e2e.setInput('input[placeholder="任务名称，回车保存"]', '界面驱动测试任务');
    const ok2 = window.__e2e.setInput('input[type="datetime-local"]', '${dueLocal}');
    window.__e2e.pressEnter('input[placeholder="任务名称，回车保存"]');
    await new Promise(r => setTimeout(r, 800));
    const t = window.__e2e.text();
    return { ok1, ok2, taskShown: t.includes('界面驱动测试任务'), countdownShown: /\\d+分钟/.test(t) };
  })()`);

  // 4. 打开管理窗口
  log("step4: open manager");
  await evaluate(ws, `(() => window.__e2e.clickByText('全部任务') ?? window.__e2e.clickByTitle('打开任务管理'))()`);
  await sleep(2500);
  // 管理窗口与悬浮窗共用 index.html（document.title 相同），
  // 通过 Tauri 内部 metadata 的窗口 label 精确识别
  const targets2 = await (await fetch(CDP_HTTP)).json();
  let manager = null;
  let ws2 = null;
  for (const t of targets2.filter((x) => x.type === "page" && x.url.includes("tauri.localhost"))) {
    const cand = await connect(t.webSocketDebuggerUrl, "cand:" + t.title);
    attach(cand);
    try {
      const label = await evaluate(cand, `window.__TAURI_INTERNALS__.metadata?.currentWindow?.label ?? 'unknown'`);
      log("page label:", label);
      if (label === "manager") {
        manager = t;
        ws2 = cand;
        break;
      }
    } catch { /* 跳过不可探测页面 */ }
    if (ws2 !== cand) cand.close();
  }
  if (manager && ws2) {
    await evaluate(ws2, HELPERS);
    results.manager = await evaluate(ws2, `(() => {
      const t = window.__e2e.text();
      return {
        hasSidebar: t.includes('全部任务') && t.includes('今日') && t.includes('已逾期'),
        hasFilters: t.includes('全部分类') && t.includes('按截止时间'),
        hasCategoriesNav: t.includes('学习') && t.includes('工作') && t.includes('会议'),
        taskListed: t.includes('界面驱动测试任务'),
        hasSettings: t.includes('设置'),
      };
    })()`);

    // 5. 在管理窗口编辑任务（双击行打开表单，改标题后保存）
    log("step5: edit via manager UI");
    results.edit = await evaluate(ws2, `(async () => {
      window.__e2e.dblclickText('界面驱动测试任务');
      await new Promise(r => setTimeout(r, 600));
      const opened = window.__e2e.text().includes('编辑任务');
      if (!opened) return { opened };
      const titleInput = [...document.querySelectorAll('input')].find(i => i.value === '界面驱动测试任务');
      if (!titleInput) return { opened, error: 'title input not found' };
      Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value').set.call(titleInput, '界面驱动测试任务（已编辑）');
      titleInput.dispatchEvent(new Event('input', { bubbles: true }));
      window.__e2e.clickByText('保存');
      await new Promise(r => setTimeout(r, 800));
      return { opened, edited: window.__e2e.text().includes('（已编辑）') };
    })()`);

    // 6. 删除任务（编辑表单中的删除按钮 + confirm 已覆写为 true）
    log("step6: delete via manager UI");
    results.delete = await evaluate(ws2, `(async () => {
      window.__e2e.dblclickText('界面驱动测试任务（已编辑）');
      await new Promise(r => setTimeout(r, 600));
      const opened = window.__e2e.text().includes('编辑任务');
      if (!opened) return { opened };
      window.__e2e.clickByText('删除');
      await new Promise(r => setTimeout(r, 800));
      return { opened, deleted: !window.__e2e.text().includes('界面驱动测试任务') };
    })()`);
    ws2.close();
  } else {
    results.manager = { error: "manager window not found" };
  }
  console.log(JSON.stringify(results, null, 2));

  const pass =
    results.layout.hasTitle && results.layout.hasStats && results.layout.hasFooter &&
    results.openAdd.panelShown &&
    results.add.taskShown && results.add.countdownShown &&
    results.manager?.taskListed && results.manager?.hasSidebar &&
    results.edit?.edited &&
    results.delete?.deleted;

  console.log(pass ? "\nE2E_UI_PASS" : "\nE2E_UI_FAIL");
  ws.close();
  process.exit(pass ? 0 : 1);
}

main().catch((e) => {
  console.error("E2E_UI_ERROR:", e.message);
  process.exit(2);
});
