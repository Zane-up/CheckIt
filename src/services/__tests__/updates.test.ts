import { afterEach, describe, expect, it, vi } from "vitest";
import { checkUpdate, compareVersions } from "../updates";

afterEach(() => vi.unstubAllGlobals());
describe("更新版本与响应校验", () => {
  it.each([["v0.2.10", "0.2.9", 1], ["0.2.2", "0.2.2", 0], ["0.2.0", "0.2.2", -1], ["1.0.0", "0.99.99", 1]])("比较 %s 和 %s", (a, b, expected) => expect(compareVersions(a, b)).toBe(expected));
  it.each(["0.3.0-beta", "latest", "0.02.0", "1.2", "<script>"])("拒绝非正式版本 %s", (tag) => expect(() => compareVersions(tag, "0.2.2")).toThrow());
  it("查询不携带凭据，必须存在安装包", async () => {
    const fetcher = vi.fn().mockResolvedValue({ ok: true, json: async () => ({ tag_name: "v0.3.0", draft: false, prerelease: false, assets: [{ name: "CheckIt_0.3.0_x64-setup.exe", size: 123 }] }) });
    vi.stubGlobal("fetch", fetcher);
    expect(await checkUpdate("0.2.2", new AbortController().signal)).toEqual({ version: "0.3.0", comparison: 1 });
    expect(fetcher.mock.calls[0][1].credentials).toBe("omit");
  });
  it("缺少安装包不谎报可更新", async () => {
    vi.stubGlobal("fetch", vi.fn().mockResolvedValue({ ok: true, json: async () => ({ tag_name: "v0.3.0", draft: false, prerelease: false, assets: [] }) }));
    await expect(checkUpdate("0.2.2", new AbortController().signal)).rejects.toThrow("安装包尚未就绪");
  });
  it("限流明确提示", async () => {
    vi.stubGlobal("fetch", vi.fn().mockResolvedValue({ ok: false, status: 403 }));
    await expect(checkUpdate("0.2.2", new AbortController().signal)).rejects.toThrow("频率受限");
  });
});
