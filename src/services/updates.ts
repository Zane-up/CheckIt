import { tr } from "@/i18n";
const RELEASE_API = "https://api.github.com/repos/Zane-up/CheckIt/releases/latest";

function stableVersion(value: unknown): number[] {
  if (typeof value !== "string" || !/^v?(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)$/.test(value)) {
    throw new Error(tr("版本信息格式异常，请前往下载页确认。"));
  }
  const parts = value.replace(/^v/, "").split(".").map(Number);
  if (!parts.every(Number.isSafeInteger)) throw new Error(tr("版本号超出支持范围。"));
  return parts;
}

export function compareVersions(latest: string, current: string): number {
  const a = stableVersion(latest);
  const b = stableVersion(current);
  for (let i = 0; i < 3; i++) {
    if (a[i] !== b[i]) return a[i] > b[i] ? 1 : -1;
  }
  return 0;
}

/** 仅用户点击时查询公开版本；不发送任务、设置或数据库内容。 */
export async function checkUpdate(current: string, signal: AbortSignal) {
  const response = await fetch(RELEASE_API, {
    signal, credentials: "omit", cache: "no-store", redirect: "error",
    headers: { Accept: "application/vnd.github+json" },
  });
  if (response.status === 403 || response.status === 429) throw new Error(tr("GitHub 查询频率受限，请稍后重试或打开下载页。"));
  if (!response.ok) throw new Error(tr("检查更新失败（HTTP {0}），请稍后重试。", response.status));
  const release = await response.json();
  if (!release || release.draft !== false || release.prerelease !== false) throw new Error(tr("暂未找到正式版本，请前往下载页确认。"));
  const comparison = compareVersions(release.tag_name, current);
  const installer = `CheckIt_${release.tag_name.replace(/^v/, "")}_x64-setup.exe`;
  if (!Array.isArray(release.assets) || !release.assets.some((asset: { name?: string; size?: number }) => asset?.name === installer && typeof asset.size === "number" && asset.size > 0)) {
    throw new Error(tr("最新版本的 Windows 安装包尚未就绪，请稍后重试。"));
  }
  return { version: release.tag_name.replace(/^v/, "") as string, comparison };
}
