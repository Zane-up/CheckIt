import { fireEvent,render,screen,waitFor } from "@testing-library/react";
import { beforeEach,expect,it,vi } from "vitest";
const mocks=vi.hoisted(()=>({list:vi.fn(),restore:vi.fn(),reload:vi.fn(),state:{tasks:[]}}));
vi.mock("@/services/api",()=>({api:{getDeletedTasks:mocks.list,restoreTask:mocks.restore}}));
vi.mock("@/hooks/useAppData",()=>({useAppData:()=>({state:mocks.state,reload:mocks.reload})}));
import TrashPanel from "../TrashPanel";
beforeEach(()=>{
  vi.clearAllMocks();mocks.reload.mockResolvedValue(undefined);
  mocks.list.mockResolvedValue([{task:{id:12,title:"误删的任务"},deleted_at:"2026-10-29T00:00:00Z"}]);
});
it("恢复精确任务并刷新共享数据",async()=>{
  mocks.restore.mockResolvedValue(undefined);render(<TrashPanel/>);
  await screen.findByText("误删的任务");fireEvent.click(screen.getByText("恢复任务"));
  await waitFor(()=>expect(screen.queryByText("误删的任务")).toBeNull());
  expect(mocks.restore).toHaveBeenCalledWith(12);expect(mocks.reload).toHaveBeenCalledOnce();
});
it("恢复失败保留条目和错误，允许重试",async()=>{
  mocks.restore.mockRejectedValueOnce(new Error("恢复失败"));render(<TrashPanel/>);
  await screen.findByText("误删的任务");fireEvent.click(screen.getByText("恢复任务"));
  expect((await screen.findByRole("alert")).textContent).toContain("恢复失败");
  expect(screen.getByText("误删的任务")).toBeTruthy();expect(mocks.reload).not.toHaveBeenCalled();
  await waitFor(()=>expect((screen.getByText("恢复任务") as HTMLButtonElement).disabled).toBe(false));
});
