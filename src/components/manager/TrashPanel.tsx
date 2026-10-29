import { useEffect,useState } from "react";
import { api } from "@/services/api";
import { useAppData } from "@/hooks/useAppData";
import { dateLocale,errorText,tr,useI18n } from "@/i18n";

export default function TrashPanel(){
  useI18n();const {state,reload}=useAppData();
  const [items,setItems]=useState<Awaited<ReturnType<typeof api.getDeletedTasks>>>([]);
  const [error,setError]=useState("");const [busy,setBusy]=useState(false);
  useEffect(()=>{let disposed=false;void api.getDeletedTasks().then(value=>{if(!disposed)setItems(value);}).catch(e=>{if(!disposed)setError(String(e));});return()=>{disposed=true;};},[state.tasks]);
  return <section className="overflow-y-auto p-6">
    <h1 className="text-base font-semibold">{tr("最近删除")}</h1>
    <p className="my-3 text-xs text-[var(--text-secondary)]">{tr("删除的任务保留七天，期间停止提醒。恢复只保留未来提醒，不补发已经过去的通知。")}</p>
    {items.length===0&&<p className="text-sm">{tr("暂无最近删除的任务")}</p>}
    <div className="space-y-2">{items.map(item=><article key={item.task.id} className="glass-subtle flex items-center justify-between gap-4 rounded-xl p-3">
      <div className="min-w-0"><h2 className="break-words text-sm">{item.task.title}</h2><p className="mt-1 text-xs text-[var(--text-secondary)]">{tr("删除时间：")}{new Date(item.deleted_at).toLocaleString(dateLocale())}</p></div>
      <button type="button" disabled={busy} className="shrink-0 rounded-lg border border-[var(--glass-border)] px-3 py-2 text-xs disabled:opacity-50" onClick={()=>{
        setBusy(true);setError("");void api.restoreTask(item.task.id).then(async()=>{setItems(old=>old.filter(v=>v.task.id!==item.task.id));await reload();}).catch(e=>setError(String(e))).finally(()=>setBusy(false));
      }}>{tr("恢复任务")}</button>
    </article>)}</div>
    {error&&<p role="alert" className="mt-3 text-xs text-[var(--urgent-text)]">{errorText(error)}</p>}
  </section>;
}
