import { useEffect, useRef, useState } from "react";
import { version } from "../../../package.json";
import { api } from "@/services/api";
import { checkUpdate } from "@/services/updates";

export default function UpdateChecker() {
  const request = useRef<AbortController | null>(null);
  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState("");
  const [error, setError] = useState("");
  useEffect(() => () => { request.current?.abort(); request.current = null; }, []);

  async function check() {
    if (request.current) return;
    const controller = new AbortController();
    request.current = controller;
    setBusy(true); setError(""); setMessage("");
    const timeout = window.setTimeout(() => controller.abort(), 15000);
    controller.signal.addEventListener("abort", () => window.clearTimeout(timeout), { once: true });
    try {
      const result = await checkUpdate(version, controller.signal);
      if (request.current !== controller) return;
      setMessage(result.comparison > 0 ? `发现新版本 v${result.version}，请打开下载页下载安装。`
        : result.comparison === 0 ? "当前已是最新正式版。"
        : `当前版本高于公开正式版 v${result.version}，无需降级。`);
    } catch (e) {
      if (request.current !== controller) return;
      setError(controller.signal.aborted ? "检查超时，请检查网络或直接打开下载页。" : e instanceof TypeError ? "无法连接 GitHub，请检查网络后重试。" : String(e instanceof Error ? e.message : e));
    } finally {
      window.clearTimeout(timeout);
      if (request.current === controller) { request.current = null; setBusy(false); }
    }
  }

  return <section className="mt-6 border-t border-[var(--glass-border)] pt-3 text-[12px]">
    <div className="font-medium">CheckIt v{version}</div>
    <p className="mt-1 text-[var(--text-secondary)]">手动检查 GitHub 正式版；下载后运行安装包完成更新，任务保留在本机。</p>
    <div className="mt-3 flex flex-wrap gap-2">
      <button type="button" disabled={busy} onClick={() => void check()} className="rounded-lg border border-[var(--glass-border)] px-3 py-2 disabled:opacity-50">{busy ? "正在检查…" : "检查更新"}</button>
      <button type="button" onClick={() => { setError(""); void api.openReleasePage().catch(() => setError("无法打开浏览器，请手动访问 github.com/Zane-up/CheckIt/releases/latest。")); }} className="rounded-lg border border-[var(--glass-border)] px-3 py-2">打开官方下载页</button>
    </div>
    <p role="status" className="mt-2 text-[var(--text-secondary)]">{message}</p>
    {error && <p role="alert" className="mt-2 text-[var(--urgent-text)]">{error}</p>}
  </section>;
}
