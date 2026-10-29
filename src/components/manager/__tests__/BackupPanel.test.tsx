import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, expect, it, vi } from "vitest";
const mocks=vi.hoisted(()=>({list:vi.fn(),create:vi.fn(),restore:vi.fn(),folder:vi.fn(),status:vi.fn(),retry:vi.fn()}));
vi.mock("@/services/api",()=>({api:{listBackups:mocks.list,createBackup:mocks.create,restoreBackup:mocks.restore,openBackupFolder:mocks.folder,getBackupStatus:mocks.status,retryAutoBackup:mocks.retry}}));
import BackupPanel from "../BackupPanel";
beforeEach(()=>{vi.clearAllMocks();mocks.list.mockResolvedValue([{name:"checkit-test.db",bytes:100}]);mocks.create.mockResolvedValue("checkit-test.db");});
it("恢复必须选择备份并二次确认，取消不调用后端",async()=>{
  render(<BackupPanel/>);await screen.findByText(/checkit-test.db ·/);
  fireEvent.change(screen.getByLabelText("选择要恢复的备份"),{target:{value:"checkit-test.db"}});
  fireEvent.click(screen.getByText("恢复所选备份…"));expect(mocks.restore).not.toHaveBeenCalled();
  fireEvent.click(screen.getByText("取消"));expect(mocks.restore).not.toHaveBeenCalled();
  fireEvent.click(screen.getByText("恢复所选备份…"));fireEvent.click(screen.getByText("确认恢复并重启"));
  await waitFor(()=>expect(mocks.restore).toHaveBeenCalledWith("checkit-test.db"));
});
it("备份失败显示错误，不显示成功",async()=>{
  mocks.create.mockRejectedValueOnce(new Error("磁盘已满"));render(<BackupPanel/>);
  await screen.findByText(/checkit-test.db ·/);fireEvent.click(screen.getByText("立即备份"));
  expect((await screen.findByRole("alert")).textContent).toContain("磁盘已满");
  expect(screen.queryByText("备份已保存到本机备份文件夹。")).toBeNull();
});
it("自动备份设置传递补丁，失败状态可通过立即重试清除",async()=>{
  mocks.status.mockResolvedValue({last_success:null,last_attempt:"2026-10-29T00:00:00Z",error:"磁盘已满"});
  mocks.retry.mockResolvedValue({last_success:"2026-10-30T00:00:00Z",last_attempt:"2026-10-30T00:00:00Z",error:null});
  const change=vi.fn();render(<BackupPanel autoEnabled keep={7} onChange={change}/>);
  expect((await screen.findByRole("alert")).textContent).toContain("磁盘已满");
  fireEvent.change(screen.getByLabelText("自动备份保留数量"),{target:{value:"14"}});
  expect(change).toHaveBeenCalledWith({auto_backup_keep:14});
  fireEvent.click(screen.getByLabelText("每天自动备份"));
  expect(change).toHaveBeenCalledWith({auto_backup_enabled:false});
  fireEvent.click(screen.getByText("立即执行自动备份"));
  await waitFor(()=>expect(screen.queryByRole("alert")).toBeNull());
  expect(mocks.retry).toHaveBeenCalledOnce();expect(mocks.list).toHaveBeenCalledTimes(2);
});
it("关闭自动备份时禁止立即执行，卸载清理轮询",async()=>{
  mocks.status.mockResolvedValue({last_success:null,last_attempt:null,error:null});
  const clear=vi.spyOn(window,"clearInterval");
  const {unmount}=render(<BackupPanel autoEnabled={false} keep={7} onChange={vi.fn()}/>);
  await screen.findByText(/上次自动备份：尚未成功备份/);
  expect((screen.getByText("立即执行自动备份") as HTMLButtonElement).disabled).toBe(true);
  expect((screen.getByLabelText("自动备份保留数量") as HTMLSelectElement).disabled).toBe(true);
  unmount();expect(clear).toHaveBeenCalled();
});
