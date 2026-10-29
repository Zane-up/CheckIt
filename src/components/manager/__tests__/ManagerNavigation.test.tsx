import { fireEvent,render,screen,waitFor } from "@testing-library/react";
import { beforeEach,expect,it,vi } from "vitest";
import type { ManagerNavigationRequest } from "@/services/api";
const mocks=vi.hoisted(()=>({request:null as ManagerNavigationRequest|null,clear:vi.fn(),get:vi.fn()}));
vi.mock("@/hooks/useManagerNavigation",()=>({useManagerNavigation:()=>({request:mocks.request,clearRequest:mocks.clear})}));
vi.mock("@/hooks/useAppData",()=>({useAppData:()=>({state:{tasks:[],categories:[]}})}));
vi.mock("@/hooks/useNow",()=>({useNow:()=>0}));
vi.mock("@/services/api",()=>({api:{getAppState:mocks.get}}));
vi.mock("../SettingsPanel",()=>({default:()=> <div>Settings content</div>}));
vi.mock("../CategoryManager",()=>({default:()=>null}));
vi.mock("../TaskListView",()=>({default:({onCreate}: {onCreate:()=>void})=><button onClick={onCreate}>Create draft</button>}));
vi.mock("../TaskFormModal",()=>({default:({task,onClose}:{task:{title:string}|null;onClose:()=>void})=><div>{task?.title??"Unsaved draft"}<button onClick={onClose}>Close editor</button></div>}));
import ManagerApp from "../ManagerApp";
beforeEach(()=>{mocks.request=null;vi.clearAllMocks();mocks.get.mockResolvedValue({tasks:[{id:8,title:"Target task"}]});});
it("首次进入直接设置页面",async()=>{
  mocks.request={id:1,target:{view:"settings"}};render(<ManagerApp/>);
  expect(await screen.findByText("Settings content")).toBeTruthy();expect(mocks.clear).toHaveBeenCalledWith(1);
});
it("任务详情从最新数据定位",async()=>{
  mocks.request={id:2,target:{view:"task",task_id:8}};render(<ManagerApp/>);
  expect(await screen.findByText("Target task")).toBeTruthy();
});
it("保留草稿，编辑关闭后再跳转",async()=>{
  const view=render(<ManagerApp/>);fireEvent.click(screen.getByText("Create draft"));
  mocks.request={id:3,target:{view:"settings"}};view.rerender(<ManagerApp/>);
  expect(screen.getByText("Unsaved draft")).toBeTruthy();expect(mocks.clear).not.toHaveBeenCalled();
  fireEvent.click(screen.getByText("Close editor"));
  expect(await screen.findByText("Settings content")).toBeTruthy();
});
it("删除后的任务给出明确提示",async()=>{
  mocks.get.mockResolvedValue({tasks:[]});mocks.request={id:4,target:{view:"task",task_id:8}};
  render(<ManagerApp/>);await waitFor(()=>expect(screen.getByRole("alert").textContent).toContain("已不存在"));
});
