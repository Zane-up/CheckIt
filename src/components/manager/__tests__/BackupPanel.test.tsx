import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, expect, it, vi } from "vitest";
const mocks=vi.hoisted(()=>({list:vi.fn(),create:vi.fn(),restore:vi.fn(),folder:vi.fn()}));
vi.mock("@/services/api",()=>({api:{listBackups:mocks.list,createBackup:mocks.create,restoreBackup:mocks.restore,openBackupFolder:mocks.folder}}));
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
