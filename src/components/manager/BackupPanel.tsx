import { dateLocale,errorText,tr, useI18n } from "@/i18n";
import { useEffect, useState } from "react";
import { api } from "@/services/api";

export default function BackupPanel({autoEnabled,keep,onChange}:{autoEnabled?:boolean;keep?:number;onChange?:(patch:{auto_backup_enabled?:boolean;auto_backup_keep?:number})=>void}) {
  useI18n();
  const [files,setFiles]=useState<{name:string;bytes:number}[]>([]);
  const [selected,setSelected]=useState("");
  const [confirm,setConfirm]=useState(false);
  const [busy,setBusy]=useState(false);
  const [message,setMessage]=useState("");
  const [error,setError]=useState("");
  const [status,setStatus]=useState<Awaited<ReturnType<typeof api.getBackupStatus>>|null>(null);
  useEffect(()=>{
    if(!onChange)return;
    let disposed=false;
    const load=()=>void api.getBackupStatus().then(v=>{if(!disposed)setStatus(v);}).catch(e=>{if(!disposed)setError(String(e));});
    load();const timer=window.setInterval(load,30000);return()=>{disposed=true;window.clearInterval(timer);};
  },[!!onChange]);
  useEffect(()=>{let disposed=false; void api.listBackups().then(v=>{if(!disposed)setFiles(v);}).catch(e=>{if(!disposed)setError(String(e));});return()=>{disposed=true;};},[]);
  async function run(action:()=>Promise<void>) {
    if(busy)return;
    setBusy(true);setError("");setMessage("");
    try{await action();}catch(e){setError(String(e));}finally{setBusy(false);}
  }
  const style="rounded-lg border border-[var(--glass-border)] px-3 py-2 text-xs disabled:opacity-40";
  return <section className="mt-4 rounded-xl border border-[var(--glass-border)] p-4 text-xs">
    <h3 className="text-[13px] font-medium">{tr("本地备份与恢复")}</h3>
    {onChange&&<div className="my-3 space-y-2 rounded-lg border border-[var(--glass-border)] p-3">
      <label className="flex items-center gap-2"><input type="checkbox" checked={autoEnabled??true} onChange={e=>onChange({auto_backup_enabled:e.target.checked})}/>{tr("每天自动备份")}</label>
      <label className="flex flex-wrap items-center gap-2">{tr("自动备份保留数量")}<select aria-label={tr("自动备份保留数量")} value={keep??7} disabled={!autoEnabled} onChange={e=>onChange({auto_backup_keep:Number(e.target.value)})} className="field-input !w-24">{[3,7,14,30].map(n=><option key={n} value={n}>{n}</option>)}</select></label>
      <p>{tr("上次自动备份：")}{status?.last_success?new Date(status.last_success).toLocaleString(dateLocale()):tr("尚未成功备份")}</p>
      <p className="text-[var(--text-secondary)]">{tr("运行期间每二十四小时备份一次，重启或睡眠恢复后补做一次；只轮换自动备份，手动和保险备份不自动删除。")}</p>
      {status?.error&&<p role="alert" className="text-[var(--urgent-text)]">{tr("自动备份失败：")}{errorText(status.error)}</p>}
      <button type="button" className="rounded-lg border border-[var(--glass-border)] px-3 py-2 disabled:opacity-40" disabled={busy||!autoEnabled} onClick={()=>void run(async()=>{const value=await api.retryAutoBackup();setStatus(value);setFiles(await api.listBackups());})}>{tr("立即执行自动备份")}</button>
    </div>}
    <p className="mt-1 leading-relaxed text-[var(--text-secondary)]">{tr("备份包含任务、分类、设置和提醒记录。升级数据库、恢复备份前会自动备份。建议通过“打开备份文件夹”另存一份到其他磁盘。")}</p>
    <div className="my-3 flex flex-wrap gap-2">
      <button className={style} disabled={busy} onClick={()=>void run(async()=>{const name=await api.createBackup();setFiles(await api.listBackups());setSelected(name);setMessage(tr("备份已保存到本机备份文件夹。"));})}>{tr("立即备份")}</button>
      <button className={style} disabled={busy} onClick={()=>void run(async()=>{await api.openBackupFolder();})}>{tr("打开备份文件夹")}</button>
      <button className={style} disabled={busy} onClick={()=>void run(async()=>{setFiles(await api.listBackups());})}>{tr("刷新列表")}</button>
    </div>
    <label className="block">{tr("选择要恢复的备份")}<select aria-label={tr("选择要恢复的备份")} className="field-input mt-2 !w-full" disabled={busy} value={selected} onChange={e=>{setSelected(e.target.value);setConfirm(false);}}>
        <option value="">{tr("请选择备份（")}{files.length} {tr("份）")}</option>
        {files.map(f=><option key={f.name} value={f.name}>{f.name} · {Math.ceil(f.bytes/1024)} KB</option>)}
      </select>
    </label>
    {!confirm ? <button className={`${style} mt-2`} disabled={busy||!selected} onClick={()=>setConfirm(true)}>{tr("恢复所选备份…")}</button> : <div className="mt-3 rounded-lg border border-[var(--glass-border)] p-3">
      <p>{tr("恢复将替换当前任务、设置和提醒记录，并重启 CheckIt。恢复前会保存当前数据的保险备份。确定恢复")}{selected}？</p>
      <div className="mt-2 flex gap-2"><button className={style} disabled={busy} onClick={()=>void run(async()=>{setMessage(tr("正在恢复，应用即将重启…"));await api.restoreBackup(selected);})}>{tr("确认恢复并重启")}</button><button className={style} disabled={busy} onClick={()=>setConfirm(false)}>{tr("取消")}</button></div>
    </div>}
    <p role="status" className="mt-2 text-[var(--text-secondary)]">{busy ? tr("正在处理，请稍候…") : message}</p>
    {error&&<p role="alert" className="mt-2 break-words text-[var(--urgent-text)]">{error}</p>}
  </section>;
}
