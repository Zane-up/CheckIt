import { useEffect, useState } from "react";
import { api } from "@/services/api";

export default function BackupPanel() {
  const [files,setFiles]=useState<{name:string;bytes:number}[]>([]);
  const [selected,setSelected]=useState("");
  const [confirm,setConfirm]=useState(false);
  const [busy,setBusy]=useState(false);
  const [message,setMessage]=useState("");
  const [error,setError]=useState("");
  useEffect(()=>{let disposed=false; void api.listBackups().then(v=>{if(!disposed)setFiles(v);}).catch(e=>{if(!disposed)setError(String(e));});return()=>{disposed=true;};},[]);
  async function run(action:()=>Promise<void>) {
    if(busy)return;
    setBusy(true);setError("");setMessage("");
    try{await action();}catch(e){setError(String(e));}finally{setBusy(false);}
  }
  const style="rounded-lg border border-[var(--glass-border)] px-3 py-2 text-xs disabled:opacity-40";
  return <section className="mt-4 rounded-xl border border-[var(--glass-border)] p-4 text-xs">
    <h3 className="text-[13px] font-medium">本地备份与恢复</h3>
    <p className="mt-1 leading-relaxed text-[var(--text-secondary)]">备份包含任务、分类、设置和提醒记录。升级数据库、恢复备份前会自动备份。建议通过“打开备份文件夹”另存一份到其他磁盘。</p>
    <div className="my-3 flex flex-wrap gap-2">
      <button className={style} disabled={busy} onClick={()=>void run(async()=>{const name=await api.createBackup();setFiles(await api.listBackups());setSelected(name);setMessage("备份已保存到本机备份文件夹。");})}>立即备份</button>
      <button className={style} disabled={busy} onClick={()=>void run(async()=>{await api.openBackupFolder();})}>打开备份文件夹</button>
      <button className={style} disabled={busy} onClick={()=>void run(async()=>{setFiles(await api.listBackups());})}>刷新列表</button>
    </div>
    <label className="block">选择要恢复的备份
      <select aria-label="选择要恢复的备份" className="field-input mt-2 !w-full" disabled={busy} value={selected} onChange={e=>{setSelected(e.target.value);setConfirm(false);}}>
        <option value="">请选择备份（{files.length} 份）</option>
        {files.map(f=><option key={f.name} value={f.name}>{f.name} · {Math.ceil(f.bytes/1024)} KB</option>)}
      </select>
    </label>
    {!confirm ? <button className={`${style} mt-2`} disabled={busy||!selected} onClick={()=>setConfirm(true)}>恢复所选备份…</button> : <div className="mt-3 rounded-lg border border-[var(--glass-border)] p-3">
      <p>恢复将替换当前任务、设置和提醒记录，并重启 CheckIt。恢复前会保存当前数据的保险备份。确定恢复 {selected}？</p>
      <div className="mt-2 flex gap-2"><button className={style} disabled={busy} onClick={()=>void run(async()=>{setMessage("正在恢复，应用即将重启…");await api.restoreBackup(selected);})}>确认恢复并重启</button><button className={style} disabled={busy} onClick={()=>setConfirm(false)}>取消</button></div>
    </div>}
    <p role="status" className="mt-2 text-[var(--text-secondary)]">{busy ? "正在处理，请稍候…" : message}</p>
    {error&&<p role="alert" className="mt-2 break-words text-[var(--urgent-text)]">{error}</p>}
  </section>;
}
