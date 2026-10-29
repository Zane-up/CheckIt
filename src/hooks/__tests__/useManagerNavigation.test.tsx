import { StrictMode } from "react";
import { act, renderHook, waitFor } from "@testing-library/react";
import { afterEach, expect, it, vi } from "vitest";
const mocks=vi.hoisted(()=>({get:vi.fn(),ack:vi.fn(async(_id:number)=>{}),stop:vi.fn(),handler:null as null|(()=>void)}));
vi.mock("@/services/api",()=>({api:{getManagerNavigation:mocks.get,acknowledgeManagerNavigation:mocks.ack}}));
vi.mock("@tauri-apps/api/event",()=>({listen:vi.fn(async(_name:string,fn:()=>void)=>{mocks.handler=fn;return mocks.stop;})}));
import { useManagerNavigation } from "../useManagerNavigation";
afterEach(()=>{vi.clearAllMocks();});
it("StrictMode 重挂载仍收到启动前缓存的导航，并确认准确 ID",async()=>{
  mocks.get.mockResolvedValue({id:1,target:{view:"settings"}});
  const hook=renderHook(()=>useManagerNavigation(),{wrapper:StrictMode});
  await waitFor(()=>expect(hook.result.current.request?.target.view).toBe("settings"));
  expect(mocks.ack).toHaveBeenCalledWith(1);
  hook.unmount();expect(mocks.stop).toHaveBeenCalled();
});
it("异步旧响应不能覆盖新请求，取消旧请求不清除新请求",async()=>{
  mocks.get.mockResolvedValue({id:5,target:{view:"settings"}});
  const hook=renderHook(()=>useManagerNavigation());
  await waitFor(()=>expect(hook.result.current.request?.id).toBe(5));
  mocks.get.mockResolvedValue({id:6,target:{view:"task",task_id:3}});
  await act(async()=>{mocks.handler?.();});
  expect(hook.result.current.request?.id).toBe(6);
  act(()=>hook.result.current.clearRequest(5));expect(hook.result.current.request?.id).toBe(6);
  mocks.get.mockResolvedValue({id:4,target:{view:"settings"}});
  await act(async()=>{mocks.handler?.();});expect(hook.result.current.request?.id).toBe(6);
});
