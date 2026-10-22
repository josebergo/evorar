/* evorar —— 纯静轻快的解压缩工具
   状态机:空态 → (拖入压缩包)浏览态 / (拖入普通文件)压缩态
   进度与结果通过 Tauri 事件驱动 */

import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { open as dialogOpen, save as dialogSave } from "@tauri-apps/plugin-dialog";

type Entry = {
  name: string;
  size: number;
  compressed_size: number;
  is_dir: boolean;
};

type ArchiveInfo = {
  path: string;
  format: string;
  entries: Entry[];
  fileCount: number;
  totalUnpacked: number;
  suggestedDir: string;
  singleTopDir: boolean;
};

// 启动动作:双击关联文件 / 资源管理器右键菜单通过命令行回调主程序
type LaunchAction =
  | { type: "open"; paths: string[] }
  | { type: "extract"; paths: string[] }
  | { type: "extract-here"; paths: string[] }
  | { type: "compress"; paths: string[] }
  | { type: "compress-zip"; paths: string[] }
  | { type: "compress-7z"; paths: string[] };

type CompressSource = { path: string; name: string; size: number };

type TaskProgress = {
  taskId: number;
  processedBytes: number;
  totalBytes: number;
  currentFile: string;
  filesDone: number;
  filesTotal: number;
};

type TaskDone = {
  taskId: number;
  ok: boolean;
  error?: string;
  stats?: { files: number; dirs: number; bytes: number; outputDir: string };
};

// ---------- 状态 ----------
let archive: ArchiveInfo | null = null;
let compressSources: CompressSource[] = [];
let compressFormat: "zip" | "7z" = "zip";
let compressLevel: "fast" | "normal" | "max" = "normal";
let activeTask: number | null = null;
let lastOutputDir: string | null = null;
let lastExtractDest: string | null = null;

// ---------- DOM ----------
const $ = <T extends HTMLElement = HTMLElement>(sel: string) =>
  document.querySelector(sel) as T;

const emptyView = $("#empty-view");
const browseView = $("#browse-view");
const compressView = $("#compress-view");
const dropHint = $("#drop-hint");

const fmtBadge = $("#fmt-badge");
const arcTitle = $("#arc-title");
const arcMeta = $("#arc-meta");
const entryList = $("#entry-list");

const compCount = $("#comp-count");
const compSize = $("#comp-size");
const compList = $("#comp-list");

const actionBrowse = $("#action-browse");
const actionCompress = $("#action-compress");

const fmtZipBtn = $("#fmt-zip");
const fmt7zBtn = $("#fmt-7z");
const lvlBtns = {
  fast: $("#lvl-fast"),
  normal: $("#lvl-normal"),
  max: $("#lvl-max"),
};

const progressCard = $("#progress-card");
const progressBar = $("#progress-bar-fill");
const progressText = $("#progress-text");
const progressFile = $("#progress-file");
const cancelBtn = $("#cancel-btn");
const toast = $("#toast");

// ---------- 工具 ----------
function fmtSize(bytes: number): string {
  if (!Number.isFinite(bytes)) return "-";
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 ** 2) return `${(bytes / 1024).toFixed(1)} KB`;
  if (bytes < 1024 ** 3) return `${(bytes / 1024 ** 2).toFixed(1)} MB`;
  return `${(bytes / 1024 ** 3).toFixed(2)} GB`;
}

function baseName(p: string): string {
  return p.replace(/[\\/]+$/, "").split(/[\\/]/).pop() ?? p;
}

function dirName(p: string): string {
  const i = Math.max(p.lastIndexOf("\\"), p.lastIndexOf("/"));
  return i > 0 ? p.slice(0, i) : ".";
}

async function pathExists(p: string): Promise<boolean> {
  try {
    return await invoke<boolean>("path_exists", { path: p });
  } catch {
    return false;
  }
}

function extName(p: string): string {
  const b = baseName(p);
  const i = b.lastIndexOf(".");
  return i > 0 ? b.slice(i + 1).toLowerCase() : "";
}

function iconFor(name: string, isDir: boolean): string {
  if (isDir) return "📁";
  const ext = extName(name);
  if (["png", "jpg", "jpeg", "gif", "webp", "bmp", "svg"].includes(ext)) return "🖼️";
  if (["mp4", "mkv", "avi", "mov"].includes(ext)) return "🎬";
  if (["mp3", "flac", "wav", "aac"].includes(ext)) return "🎵";
  if (["pdf"].includes(ext)) return "📕";
  if (["doc", "docx", "txt", "md"].includes(ext)) return "📄";
  if (["xls", "xlsx", "csv"].includes(ext)) return "📊";
  if (["exe", "msi", "bat"].includes(ext)) return "⚙️";
  if (["zip", "7z", "rar", "tar", "gz", "bz2", "xz"].includes(ext)) return "🗜️";
  return "📄";
}

// ---------- 视图切换 ----------
function showView(v: HTMLElement) {
  emptyView.style.display = "none";
  browseView.style.display = "none";
  compressView.style.display = "none";
  v.style.display = "flex";
  actionBrowse.style.display = v === browseView ? "flex" : "none";
  actionCompress.style.display = v === compressView ? "flex" : "none";
}

function reset() {
  archive = null;
  compressSources = [];
  showView(emptyView);
}

// ---------- 打开压缩包 ----------
async function openArchive(path: string) {
  dropHint.classList.add("busy");
  try {
    const info = await invoke<ArchiveInfo>("open_archive", { path });
    archive = info;
    compressSources = [];
    renderArchive();
    showView(browseView);
  } catch (e) {
    toastShow(`打开失败:${e}`, true);
  } finally {
    dropHint.classList.remove("busy");
  }
}

function renderArchive() {
  if (!archive) return;
  fmtBadge.textContent = archive.format;
  fmtBadge.className = "badge";
  arcTitle.textContent = baseName(archive.path);
  arcTitle.title = archive.path;
  arcMeta.textContent = `${archive.fileCount} 个文件 · 解压后 ${fmtSize(archive.totalUnpacked)} · ${archive.singleTopDir ? "单顶层目录 → 智能解压不套娃" : "智能解压将创建同名文件夹"}`;
  entryList.innerHTML = "";
  const sorted = [...archive.entries].sort((a, b) => {
    if (a.is_dir !== b.is_dir) return a.is_dir ? -1 : 1;
    return a.name.localeCompare(b.name, "zh-CN");
  });
  const frag = document.createDocumentFragment();
  const max = 800; // 超大包截断渲染,保持界面流畅
  sorted.slice(0, max).forEach((e) => {
    const row = document.createElement("div");
    row.className = "entry-row";
    const name = e.is_dir ? e.name.replace(/[\/\\]$/, "") : e.name;
    row.innerHTML = `<span class="entry-ico">${iconFor(name, e.is_dir)}</span>
      <span class="entry-name" title="${name}">${name}</span>
      <span class="entry-size">${e.is_dir ? "" : fmtSize(e.size)}</span>`;
    frag.appendChild(row);
  });
  if (sorted.length > max) {
    const more = document.createElement("div");
    more.className = "entry-more";
    more.textContent = `… 其余 ${sorted.length - max} 个条目未展示(不影响解压)`;
    frag.appendChild(more);
  }
  entryList.appendChild(frag);
}

// ---------- 压缩待选 ----------
function addCompressSources(paths: string[]) {
  for (const p of paths) {
    if (!compressSources.some((c) => c.path === p)) {
      compressSources.push({ path: p, name: baseName(p), size: 0 });
    }
  }
  renderCompress();
  showView(compressView);
}

function renderCompress() {
  compCount.textContent = `${compressSources.length} 个项目`;
  const total = compressSources.reduce((s, c) => s + c.size, 0);
  compSize.textContent = total > 0 ? fmtSize(total) : "";
  compList.innerHTML = "";
  const frag = document.createDocumentFragment();
  compressSources.forEach((c) => {
    const row = document.createElement("div");
    row.className = "entry-row";
    row.innerHTML = `<span class="entry-ico">${iconFor(c.name, false)}</span>
      <span class="entry-name" title="${c.path}">${c.name}</span>
      <button class="row-remove" title="移除">✕</button>`;
    row.querySelector(".row-remove")!.addEventListener("click", () => {
      compressSources = compressSources.filter((x) => x.path !== c.path);
      if (compressSources.length === 0) {
        reset();
      } else {
        renderCompress();
      }
    });
    frag.appendChild(row);
  });
  compList.appendChild(frag);
}

// ---------- 任务 ----------
function setProgress(p: TaskProgress | null, indeterminate = false) {
  if (p === null) {
    progressCard.classList.remove("visible");
    activeTask = null;
    return;
  }
  progressCard.classList.add("visible");
  activeTask = p.taskId;
  if (indeterminate || p.totalBytes === 0) {
    progressBar.classList.add("indeterminate");
    progressBar.style.width = "100%";
    progressText.textContent = p.filesTotal
      ? `${p.filesDone}/${p.filesTotal} 个文件`
      : "处理中…";
  } else {
    progressBar.classList.remove("indeterminate");
    const pct = Math.min(100, (p.processedBytes / p.totalBytes) * 100);
    progressBar.style.width = `${pct.toFixed(1)}%`;
    progressText.textContent = `${fmtSize(p.processedBytes)} / ${fmtSize(p.totalBytes)}(${pct.toFixed(0)}%)`;
  }
  progressFile.textContent = p.currentFile || "";
}

let toastTimer: ReturnType<typeof setTimeout> | null = null;
function toastShow(msg: string, isError = false, outputDir?: string) {
  toast.className = `toast visible ${isError ? "error" : ""}`;
  toast.innerHTML = "";
  const span = document.createElement("span");
  span.textContent = msg;
  toast.appendChild(span);
  if (outputDir) {
    const btn = document.createElement("button");
    btn.className = "toast-action";
    btn.textContent = "打开文件夹";
    btn.addEventListener("click", () => {
      invoke("reveal_path", { path: outputDir }).catch(() => {});
    });
    toast.appendChild(btn);
  }
  if (toastTimer) clearTimeout(toastTimer);
  toastTimer = setTimeout(() => toast.classList.remove("visible"), isError ? 6000 : 4000);
}

// ---------- 解压 ----------
async function startExtract(dest: string, password?: string) {
  if (!archive) return;
  lastExtractDest = dest;
  try {
    const taskId = await invoke<number>("extract_archive", {
      path: archive.path,
      dest,
      password: password ?? null,
    });
    setProgress({ taskId, processedBytes: 0, totalBytes: 0, currentFile: "", filesDone: 0, filesTotal: 0 }, true);
  } catch (e) {
    toastShow(`无法开始解压:${e}`, true);
  }
}

$("#smart-extract-btn")!.addEventListener("click", () => {
  if (archive) startExtract(archive.suggestedDir);
});

$("#extract-to-btn")!.addEventListener("click", async () => {
  if (!archive) return;
  const dir = await dialogOpen({
    directory: true,
    multiple: false,
    title: "选择解压位置",
    defaultPath: archive.path ? undefined : undefined,
  });
  if (typeof dir === "string" && dir) startExtract(dir);
});

// ---------- 压缩 ----------
function refreshFormatButtons() {
  fmtZipBtn.classList.toggle("active", compressFormat === "zip");
  fmt7zBtn.classList.toggle("active", compressFormat === "7z");
  (lvlBtns.fast as HTMLElement).classList.toggle("active", compressLevel === "fast");
  (lvlBtns.normal as HTMLElement).classList.toggle("active", compressLevel === "normal");
  (lvlBtns.max as HTMLElement).classList.toggle("active", compressLevel === "max");
}
fmtZipBtn.addEventListener("click", () => {
  compressFormat = "zip";
  refreshFormatButtons();
});
fmt7zBtn.addEventListener("click", () => {
  compressFormat = "7z";
  refreshFormatButtons();
});
Object.entries(lvlBtns).forEach(([key, btn]) => {
  btn.addEventListener("click", () => {
    compressLevel = key as "fast" | "normal" | "max";
    refreshFormatButtons();
  });
});
refreshFormatButtons();

$("#compress-btn")!.addEventListener("click", async () => {
  if (compressSources.length === 0) return;
  const first = compressSources[0];
  const suggest =
    compressSources.length === 1
      ? first.name.replace(/\.[^.]+$/, "")
      : "新建压缩包";
  const dest = await dialogSave({
    title: "保存压缩包",
    defaultPath: `${suggest}.${compressFormat === "zip" ? "zip" : "7z"}`,
    filters: [
      { name: compressFormat === "zip" ? "ZIP 压缩包" : "7Z 压缩包", extensions: [compressFormat] },
    ],
  });
  if (typeof dest !== "string" || !dest) return;
  try {
    const taskId = await invoke<number>("compress_files", {
      args: {
        sources: compressSources.map((c) => c.path),
        dest,
        format: compressFormat,
        level: compressLevel,
      },
      password: null,
    });
    setProgress({ taskId, processedBytes: 0, totalBytes: 0, currentFile: "", filesDone: 0, filesTotal: 0 }, true);
  } catch (e) {
    toastShow(`无法开始压缩:${e}`, true);
  }
});

// ---------- 启动动作(关联双击 / 右键菜单) ----------
// 右键「压缩为 ZIP/7Z」:落盘到同目录,重名自动加序号
async function uniqueDest(candidate: string): Promise<string> {
  if (!(await pathExists(candidate))) return candidate;
  const slash = Math.max(candidate.lastIndexOf("\\"), candidate.lastIndexOf("/"));
  const dot = candidate.lastIndexOf(".");
  const hasExt = dot > slash;
  const stem = hasExt ? candidate.slice(0, dot) : candidate;
  const ext = hasExt ? candidate.slice(dot) : "";
  for (let i = 2; i < 1000; i++) {
    const next = `${stem} (${i})${ext}`;
    if (!(await pathExists(next))) return next;
  }
  return `${stem} (${Date.now()})${ext}`;
}

async function quickCompress(paths: string[], format: "zip" | "7z") {
  addCompressSources(paths);
  compressFormat = format;
  refreshFormatButtons();
  const parent = dirName(paths[0]);
  const stem =
    paths.length === 1
      ? baseName(paths[0]).replace(/\.[^.]+$/, "") || baseName(paths[0])
      : "新建压缩包";
  const dest = await uniqueDest(`${parent}\\${stem}.${format}`);
  try {
    const taskId = await invoke<number>("compress_files", {
      args: { sources: paths, dest, format, level: "normal" },
      password: null,
    });
    setProgress({ taskId, processedBytes: 0, totalBytes: 0, currentFile: "", filesDone: 0, filesTotal: 0 }, true);
  } catch (e) {
    toastShow(`无法开始压缩:${e}`, true);
  }
}

// 多选压缩包右键解压时逐个处理,复用现有进度/取消/完成提示
const extractQueue: { path: string; here: boolean }[] = [];
let queueBusy = false;

async function pumpExtractQueue() {
  if (queueBusy) return;
  const next = extractQueue.shift();
  if (!next) return;
  queueBusy = true;
  await openArchive(next.path);
  if (!archive) {
    queueBusy = false;
    pumpExtractQueue();
    return;
  }
  const dest = next.here ? dirName(archive.path) : archive.suggestedDir;
  startExtract(dest);
}

async function handleLaunch(action: LaunchAction) {
  const paths = action.paths ?? [];
  if (paths.length === 0) return;
  switch (action.type) {
    case "open":
      await openArchive(paths[0]);
      break;
    case "extract":
    case "extract-here":
      for (const p of paths) {
        extractQueue.push({ path: p, here: action.type === "extract-here" });
      }
      pumpExtractQueue();
      break;
    case "compress":
      addCompressSources(paths);
      break;
    case "compress-zip":
      await quickCompress(paths, "zip");
      break;
    case "compress-7z":
      await quickCompress(paths, "7z");
      break;
  }
}

// 首次启动:命令行携带的动作(页面就绪后取走);运行中:单实例插件转发
invoke<LaunchAction | null>("get_pending_launch").then((a) => {
  if (a) handleLaunch(a);
});
listen<LaunchAction>("launch-action", (ev) => handleLaunch(ev.payload));

// ---------- 取消 ----------
cancelBtn.addEventListener("click", () => {
  if (activeTask != null) {
    invoke("cancel_task", { taskId: activeTask }).catch(() => {});
    toastShow("正在取消…");
  }
});

// ---------- 事件(Tauri) ----------
listen<TaskProgress>("task-progress", (ev) => {
  setProgress(ev.payload);
});

listen<TaskDone>("task-done", (ev) => {
  const d = ev.payload;
  setProgress(null);
  queueBusy = false;
  const moreQueued = extractQueue.length > 0;
  if (d.ok && d.stats) {
    lastOutputDir = d.stats.outputDir;
    const target = d.stats.outputDir;
    const isCompress = compressSources.length > 0 && archive === null;
    toastShow(
      isCompress
        ? `压缩完成:${d.stats.files} 个文件`
        : `解压完成:${d.stats.files} 个文件 · ${fmtSize(d.stats.bytes)}`,
      false,
      target,
    );
  } else {
    const err = d.error ?? "未知错误";
    if (err.includes("密码") || err.toLowerCase().includes("password")) {
      const pw = prompt("该压缩包已加密,请输入密码:");
      if (pw && archive) {
        startExtract(lastExtractDest ?? archive.suggestedDir, pw);
        return;
      }
    }
    toastShow(`失败:${err}`, true);
  }
  if (moreQueued) pumpExtractQueue();
});

// 拖放:Tauri 原生事件提供绝对路径
listen<{ paths: string[] }>("tauri://drag-drop", async (ev) => {
  const paths = ev.payload.paths ?? [];
  if (paths.length === 0) return;
  document.body.classList.remove("dragging");
  // 分类:压缩包 → 打开;普通文件/目录 → 压缩待选
  const archives: string[] = [];
  const plain: string[] = [];
  for (const p of paths) {
    const known =
      ["zip", "7z", "rar", "tar", "gz", "tgz", "bz2", "tbz2", "xz", "txz", "tgz"].includes(
        extName(p),
      ) || /\.(tar\.(gz|bz2|xz))$/i.test(p);
    if (!known) {
      plain.push(p);
      continue;
    }
    try {
      const ok = await invoke<boolean>("is_archive", { path: p });
      if (ok) archives.push(p);
      else plain.push(p);
    } catch {
      plain.push(p);
    }
  }
  if (archives.length > 0) {
    await openArchive(archives[0]);
    if (plain.length > 0) toastShow(`已打开压缩包;${plain.length} 个普通文件待压缩列表未加入`);
  } else if (plain.length > 0) {
    addCompressSources(plain);
  }
});

listen("tauri://drag-enter", () => document.body.classList.add("dragging"));
listen("tauri://drag-leave", () => document.body.classList.remove("dragging"));

// 空态点击也允许手动选择文件
$("#pick-archive-btn")!.addEventListener("click", async () => {
  const files = await dialogOpen({
    multiple: false,
    title: "打开压缩包",
    filters: [
      {
        name: "压缩包",
        extensions: ["zip", "7z", "rar", "tar", "gz", "tgz", "bz2", "xz", "tbz2", "txz"],
      },
    ],
  });
  if (typeof files === "string" && files) openArchive(files);
});

$("#pick-files-btn")!.addEventListener("click", async () => {
  const files = await dialogOpen({ multiple: true, title: "选择要压缩的文件" });
  if (Array.isArray(files) && files.length > 0) addCompressSources(files);
});

// 回到空态
$("#home-btn")!.addEventListener("click", reset);

// ---------- 关于弹窗 ----------
const GITHUB_URL = "https://github.com/josebergo/evorar";
const aboutModal = $("#about-modal");

$("#menu-about")!.addEventListener("click", async () => {
  try {
    const ver = await invoke<string>("app_version");
    $("#about-version")!.textContent = `v${ver}`;
  } catch {
    $("#about-version")!.textContent = "";
  }
  aboutModal.classList.add("visible");
});

function closeAbout() {
  aboutModal.classList.remove("visible");
}

$("#about-close")!.addEventListener("click", closeAbout);
aboutModal.addEventListener("click", (e) => {
  if (e.target === aboutModal) closeAbout();
});

$("#about-github")!.addEventListener("click", () => {
  invoke("open_url", { url: GITHUB_URL }).catch((e) => toastShow(`打开链接失败:${e}`, true));
});

// ---------- 系统集成(文件关联 / 右键菜单) ----------
type Integration = { association: boolean; contextMenu: boolean };
const settingsModal = $("#settings-modal");
const assocToggle = $("#assoc-toggle") as HTMLButtonElement;
const ctxToggle = $("#ctx-toggle") as HTMLButtonElement;

async function refreshIntegration(): Promise<Integration> {
  let cur: Integration = { association: false, contextMenu: false };
  try {
    cur = await invoke<Integration>("shell_integration_get");
  } catch {
    // 注册表不可读时保持关闭态展示
  }
  assocToggle.classList.toggle("on", cur.association);
  assocToggle.setAttribute("aria-checked", String(cur.association));
  ctxToggle.classList.toggle("on", cur.contextMenu);
  ctxToggle.setAttribute("aria-checked", String(cur.contextMenu));
  return cur;
}

async function applyIntegration(next: Integration, btn: HTMLButtonElement) {
  btn.disabled = true;
  try {
    await invoke("shell_integration_set", next);
    toastShow("已更新系统关联");
  } catch (e) {
    toastShow(`更新失败:${e}`, true);
  }
  await refreshIntegration();
  btn.disabled = false;
}

assocToggle.addEventListener("click", async () => {
  const cur = await refreshIntegration();
  await applyIntegration({ ...cur, association: !cur.association }, assocToggle);
});

ctxToggle.addEventListener("click", async () => {
  const cur = await refreshIntegration();
  await applyIntegration({ ...cur, contextMenu: !cur.contextMenu }, ctxToggle);
});

function closeSettings() {
  settingsModal.classList.remove("visible");
}

$("#menu-settings")!.addEventListener("click", () => {
  refreshIntegration();
  settingsModal.classList.add("visible");
});
$("#settings-close")!.addEventListener("click", closeSettings);
settingsModal.addEventListener("click", (e) => {
  if (e.target === settingsModal) closeSettings();
});

// 初始视图
showView(emptyView);
