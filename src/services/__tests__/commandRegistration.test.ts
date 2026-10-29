// 跨语言契约检查，直接读取生产入口，防止前端 mock 掩盖命令漏注册。
// 此测试不替代安装版的真实 WebView IPC 验收。
import { expect, it } from "vitest";
import apiSource from "../api.ts?raw";
import entry from "../../../src-tauri/src/lib.rs?raw";

it("前端调用的每一个命令都注册在生产 Tauri handler 中", () => {
  const handler = entry.match(/\.invoke_handler\(tauri::generate_handler!\[([\s\S]*?)\]\)/)?.[1];
  expect(handler, "未找到生产 invoke_handler").toBeDefined();
  const registered = new Set([...handler!.matchAll(/(?:commands|popup)::(\w+)/g)].map((m) => m[1]));
  const invoked = [...apiSource.matchAll(/\binvoke(?:<[^>]+>)?\(\s*"([^"]+)"/g)].map((m) => m[1]);
  expect(invoked.length).toBeGreaterThan(0);
  expect(invoked.filter((name) => !registered.has(name)), "以下前端命令未注册").toEqual([]);
});
