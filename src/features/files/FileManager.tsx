import { useShallow } from "zustand/react/shallow";
import {
  AlertCircle,
  Archive,
  ArrowDownToLine,
  ArrowLeft,
  ArrowUpToLine,
  Check,
  ChevronDown,
  ChevronRight,
  Clipboard,
  Columns3,
  File,
  FileCode2,
  FilePlus,
  Folder,
  FolderPlus,
  FolderRoot,
  House,
  LoaderCircle,
  MoreHorizontal,
  PanelLeftClose,
  PanelLeftOpen,
  PackageOpen,
  Pencil,
  RefreshCw,
  RotateCcw,
  Search,
  Trash2,
  X,
} from "lucide-react";
import {
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
  type CSSProperties,
  type KeyboardEvent as ReactKeyboardEvent,
  type PointerEvent as ReactPointerEvent,
} from "react";
import { open, save } from "@tauri-apps/plugin-dialog";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { api } from "../../lib/api";
import { useAppStore } from "../../store/app-store";
import type { ConflictPolicy, RemoteFile, TerminalSession } from "../../types";
import { IconButton } from "../../components/IconButton";
import { errorMessage, formatBytes } from "../../lib/format";
import { waitForTask } from "../../lib/background-task";
import { virtualWindow } from "../../lib/runtime-metrics";
import { workspaceRuntime } from "../../lib/workspace-runtime";
import { RemoteDirectoryTree } from "./RemoteDirectoryTree";
import { usePlatformCapabilities } from "../../lib/platform";
import { externalApplicationDialogOptions } from "./external-application";
import { joinLocalPath, localPathName } from "../../lib/local-path";
import { nativeDropIsInsideElement } from "./native-file-drop";
import {
  DIRECTORY_REQUEST_TIMEOUT_MS,
  withTimeout,
} from "../../lib/async-timeout";
import { parseRemoteMode } from "./file-permissions";
import { PanelFontSizeControl } from "../../components/PanelFontSizeControl";
import {
  panelFontSizeStorageKeys,
  usePanelFontSize,
} from "../../lib/panel-font-size";
import "./FileManager.css";

type SortKey = "name" | "size" | "modifiedAt";
type FileColumn = "size" | "kind" | "modifiedAt" | "permissions" | "owner";
type FileTableColumn = "name" | FileColumn;
type FileColumnWidths = Record<FileTableColumn, number>;
type RemoteBreadcrumb = {
  label: string;
  path: string;
  collapsed?: boolean;
};

const defaultFileColumns: FileColumn[] = [
  "size",
  "kind",
  "modifiedAt",
  "permissions",
  "owner",
];

export function FileManager({ session }: { session: TerminalSession }) {
  const platform = usePlatformCapabilities();
  const [
    fileFontSize,
    setFileFontSize,
    useAutomaticFileFontSize,
    fileFontSizeAutomatic,
  ] = usePanelFontSize(panelFontSizeStorageKeys.files);
  const restored = workspaceRuntime.remoteFileBrowserBySession.get(session.id);
  const { settings, setError, setPanel, openTextEditor, addTransfer } =
    useAppStore(
      useShallow((state) => ({
        settings: state.settings,
        setError: state.setError,
        setPanel: state.setPanel,
        openTextEditor: state.openTextEditor,
        addTransfer: state.addTransfer,
      })),
    );
  const [path, setPath] = useState(restored?.path ?? "/");
  const [draftPath, setDraftPath] = useState(restored?.path ?? "/");
  const [pathEditing, setPathEditing] = useState(false);
  const [pathSubmitting, setPathSubmitting] = useState(false);
  const [pathEditError, setPathEditError] = useState<string | null>(null);
  const [remoteHomePath, setRemoteHomePath] = useState<string | null>(null);
  const [homeLoading, setHomeLoading] = useState(false);
  const [files, setFiles] = useState<RemoteFile[]>([]);
  const [loading, setLoading] = useState(false);
  const [directoryError, setDirectoryError] = useState<string | null>(null);
  const [fileQuery, setFileQuery] = useState("");
  const [treeOpen, setTreeOpen] = useState(
    () => localStorage.getItem("cnshell-file-tree-open") !== "0",
  );
  const [visibleColumns, setVisibleColumns] = useState<Set<FileColumn>>(
    () => new Set(readFileColumns()),
  );
  const [columnWidths, setColumnWidths] =
    useState<FileColumnWidths>(readFileColumnWidths);
  const [selected, setSelected] = useState<RemoteFile | null>(null);
  const [sort, setSort] = useState<{ key: SortKey; asc: boolean }>({
    key: "name",
    asc: true,
  });
  const setEditor = (path: string) =>
    openTextEditor({
      sessionId: session.id,
      connectionId: session.connectionId,
      path,
    });
  const [dragging, setDragging] = useState(false);
  const [scrollTop, setScrollTop] = useState(0);
  const [viewportHeight, setViewportHeight] = useState(260);
  const [background, setBackground] = useState<{
    id: string;
    label: string;
  } | null>(null);
  const [contextMenu, setContextMenu] = useState<{
    x: number;
    y: number;
  } | null>(null);
  const [actionsOpen, setActionsOpen] = useState(false);
  const bodyRef = useRef<HTMLDivElement>(null);
  const browserRef = useRef<HTMLDivElement>(null);
  const actionsRef = useRef<HTMLDivElement>(null);
  const pathInputRef = useRef<HTMLInputElement>(null);
  const loadRequestRef = useRef(0);
  const listRequestsRef = useRef(new Map<string, Promise<RemoteFile[]>>());
  const uploadPathsRef = useRef<(paths: string[]) => Promise<void>>(
    async () => {},
  );
  const beginPathEditing = useCallback(() => {
    setDraftPath(path);
    setPathEditError(null);
    setPathEditing(true);
  }, [path]);
  const cancelPathEditing = useCallback(() => {
    setDraftPath(path);
    setPathEditError(null);
    setPathEditing(false);
  }, [path]);
  useEffect(() => {
    if (!pathEditing) return;
    pathInputRef.current?.focus();
    pathInputRef.current?.select();
  }, [pathEditing]);
  const rememberBrowser = useCallback(
    (patch: Partial<{ path: string; expandedPaths: string[] }>) => {
      const current = workspaceRuntime.remoteFileBrowserBySession.get(
        session.id,
      ) ?? { path: "/", expandedPaths: ["/"] };
      workspaceRuntime.remoteFileBrowserBySession.set(session.id, {
        ...current,
        ...patch,
      });
    },
    [session.id],
  );
  const rememberExpanded = useCallback(
    (expandedPaths: string[]) => rememberBrowser({ expandedPaths }),
    [rememberBrowser],
  );
  const listRemoteFiles = useCallback(
    (target: string) => {
      const key = `${settings.showHiddenFiles ? "hidden" : "visible"}:${target}`;
      const existing = listRequestsRef.current.get(key);
      if (existing) return existing;
      const request = api
        .listFiles(session.id, target, settings.showHiddenFiles)
        .finally(() => {
          if (listRequestsRef.current.get(key) === request)
            listRequestsRef.current.delete(key);
        });
      listRequestsRef.current.set(key, request);
      return request;
    },
    [session.id, settings.showHiddenFiles],
  );
  const load = useCallback(async () => {
    const requestId = ++loadRequestRef.current;
    setLoading(true);
    setDirectoryError(null);
    try {
      const result = await withTimeout(
        listRemoteFiles(path),
        DIRECTORY_REQUEST_TIMEOUT_MS,
        `目录读取 ${path} 超时，请重试`,
      );
      if (requestId !== loadRequestRef.current) return;
      setFiles(result);
      setDraftPath(path);
      setSelected(null);
      setScrollTop(0);
      if (bodyRef.current) bodyRef.current.scrollTop = 0;
    } catch (reason) {
      if (requestId !== loadRequestRef.current) return;
      const message = errorMessage(reason);
      setDirectoryError(message);
      setError(message);
    } finally {
      if (requestId === loadRequestRef.current) setLoading(false);
    }
  }, [listRemoteFiles, path, setError]);
  useEffect(() => {
    void load();
    return () => {
      loadRequestRef.current += 1;
    };
  }, [load]);
  const sorted = useMemo(
    () =>
      files
        .filter((item) =>
          `${item.name} ${item.kind} ${item.permissions} ${item.owner ?? ""} ${item.group ?? ""}`
            .toLocaleLowerCase("zh-CN")
            .includes(fileQuery.trim().toLocaleLowerCase("zh-CN")),
        )
        .sort((a, b) => {
          if (a.kind !== b.kind) return a.kind === "directory" ? -1 : 1;
          const direction = sort.asc ? 1 : -1;
          if (sort.key === "size") return (a.size - b.size) * direction;
          if (sort.key === "modifiedAt")
            return ((a.modifiedAt ?? 0) - (b.modifiedAt ?? 0)) * direction;
          return a.name.localeCompare(b.name, "zh-CN") * direction;
        }),
    [fileQuery, files, sort],
  );
  useEffect(
    () => localStorage.setItem("cnshell-file-tree-open", treeOpen ? "1" : "0"),
    [treeOpen],
  );
  useEffect(
    () =>
      localStorage.setItem(
        "cnshell-file-columns",
        JSON.stringify([...visibleColumns]),
      ),
    [visibleColumns],
  );
  useEffect(
    () =>
      localStorage.setItem(
        "cnshell-file-column-widths",
        JSON.stringify(columnWidths),
      ),
    [columnWidths],
  );
  const windowRange = virtualWindow(sorted.length, scrollTop, viewportHeight);
  const visibleFiles = sorted.slice(windowRange.start, windowRange.end);
  useEffect(() => {
    const body = bodyRef.current;
    if (!body) return;
    const resize = new ResizeObserver(() =>
      setViewportHeight(body.clientHeight),
    );
    resize.observe(body);
    setViewportHeight(body.clientHeight);
    return () => resize.disconnect();
  }, []);
  useEffect(() => {
    if (!contextMenu) return;
    const close = () => setContextMenu(null);
    const key = (event: KeyboardEvent) => {
      if (event.key === "Escape") close();
    };
    window.addEventListener("pointerdown", close);
    window.addEventListener("keydown", key);
    return () => {
      window.removeEventListener("pointerdown", close);
      window.removeEventListener("keydown", key);
    };
  }, [contextMenu]);
  useEffect(() => {
    if (!actionsOpen) return;
    const close = (event: PointerEvent) => {
      if (!actionsRef.current?.contains(event.target as Node))
        setActionsOpen(false);
    };
    const key = (event: KeyboardEvent) => {
      if (event.key === "Escape") setActionsOpen(false);
    };
    window.addEventListener("pointerdown", close);
    window.addEventListener("keydown", key);
    return () => {
      window.removeEventListener("pointerdown", close);
      window.removeEventListener("keydown", key);
    };
  }, [actionsOpen]);
  useEffect(() => setActionsOpen(false), [selected?.path, session.id]);
  const navigate = (target: string) => {
    const normalized = target.startsWith("cnshell-raw-path:")
      ? target
      : normalizeRemotePath(target);
    setDraftPath(normalized);
    setPathEditing(false);
    setPathEditError(null);
    setPath(normalized);
    rememberBrowser({ path: normalized });
  };
  const submitEditedPath = async () => {
    if (pathSubmitting) return;
    const normalized = draftPath.startsWith("cnshell-raw-path:")
      ? draftPath
      : normalizeRemotePath(draftPath.trim());
    if (normalized === path) {
      cancelPathEditing();
      return;
    }
    setPathSubmitting(true);
    setPathEditError(null);
    try {
      await withTimeout(
        listRemoteFiles(normalized),
        DIRECTORY_REQUEST_TIMEOUT_MS,
        `目录读取 ${normalized} 超时，请重试`,
      );
      navigate(normalized);
    } catch (reason) {
      setPathEditError(errorMessage(reason));
      pathInputRef.current?.focus();
      pathInputRef.current?.select();
    } finally {
      setPathSubmitting(false);
    }
  };
  const goHome = async () => {
    if (homeLoading) return;
    setHomeLoading(true);
    try {
      const target =
        remoteHomePath ?? (await api.remoteHomeDirectory(session.id));
      setRemoteHomePath(target);
      navigate(target);
    } catch (reason) {
      setError(`无法打开用户主目录：${errorMessage(reason)}`);
    } finally {
      setHomeLoading(false);
    }
  };
  useEffect(() => {
    const focusPath = (event: KeyboardEvent) => {
      if (
        (event.metaKey || event.ctrlKey) &&
        !event.altKey &&
        !event.shiftKey &&
        event.key.toLocaleLowerCase() === "l"
      ) {
        event.preventDefault();
        beginPathEditing();
      }
    };
    window.addEventListener("keydown", focusPath);
    return () => window.removeEventListener("keydown", focusPath);
  }, [beginPathEditing]);
  const parent = () => {
    if (path.startsWith("cnshell-raw-path:")) {
      navigate("/");
      return;
    }
    navigate(path.split("/").slice(0, -1).join("/") || "/");
  };
  const listTreeDirectories = useCallback(
    async (target: string) =>
      (await listRemoteFiles(target))
        .filter((item) => item.kind === "directory")
        .sort((left, right) => left.name.localeCompare(right.name, "zh-CN"))
        .map(({ name, path }) => ({ name, path })),
    [listRemoteFiles],
  );
  const reportTreeError = useCallback(
    (reason: unknown) => setError(errorMessage(reason)),
    [setError],
  );
  const refreshTree = () =>
    window.dispatchEvent(new Event("cnshell-refresh-directory-tree"));
  const createFolder = async (target = path) => {
    const name = prompt("新文件夹名称");
    if (!name) return;
    try {
      await api.createDirectory(
        session.id,
        await api.joinRemotePath(target, name),
      );
      await load();
      refreshTree();
    } catch (reason) {
      setError(errorMessage(reason));
    }
  };
  const createFile = async (target = path) => {
    const name = prompt("新文件名称");
    if (!name) return;
    try {
      const filePath = await api.joinRemotePath(target, name);
      await api.createText(session.id, filePath);
      await load();
      setEditor(filePath);
    } catch (reason) {
      setError(errorMessage(reason));
    }
  };
  const rename = async () => {
    if (!selected) return;
    const name = prompt("新名称", selected.name);
    if (!name || name === selected.name) return;
    try {
      await api.renameRemote(
        session.id,
        selected.path,
        await api.joinRemotePath(path, name),
      );
      await load();
      if (selected.kind === "directory") refreshTree();
    } catch (reason) {
      setError(errorMessage(reason));
    }
  };
  const remove = async () => {
    if (!selected || !confirm(`确定删除 ${selected.name}？此操作无法撤销。`))
      return;
    try {
      await api.deleteRemote(
        session.id,
        selected.path,
        selected.kind === "directory",
      );
      await load();
      if (selected.kind === "directory") refreshTree();
    } catch (reason) {
      setError(errorMessage(reason));
    }
  };
  const chmod = async () => {
    if (!selected) return;
    const value = prompt(
      "输入八进制权限，例如 755",
      selected.permissions.includes("x") ? "755" : "644",
    );
    if (value === null) return;
    const mode = parseRemoteMode(value);
    if (mode === null) {
      setError("权限必须是 3 或 4 位八进制数字，例如 644、755 或 0755");
      return;
    }
    try {
      await api.chmodRemote(session.id, selected.path, mode);
      await load();
    } catch (reason) {
      setError(errorMessage(reason));
    }
  };
  const chooseConflictPolicy = useCallback(
    (message: string): Exclude<ConflictPolicy, "ask"> => {
      const answer = prompt(
        `${message}\n输入 O 覆盖、S 跳过、R 自动重命名（默认）`,
        "R",
      )
        ?.trim()
        .toLowerCase();
      return answer === "o" ? "overwrite" : answer === "s" ? "skip" : "rename";
    },
    [],
  );
  const upload = async (target = path) => {
    if (!api.isDesktop()) {
      setError("上传需要运行 CNshell 桌面版");
      return;
    }
    const source = await open({ multiple: false, directory: false });
    if (!source) return;
    const destination = await api.joinRemotePath(target, localPathName(source));
    const conflictPolicy: ConflictPolicy = files.some(
      (item) => item.path === destination,
    )
      ? chooseConflictPolicy("远端已存在同名文件。")
      : "ask";
    try {
      addTransfer(
        await api.enqueueTransfer({
          sessionId: session.id,
          direction: "upload",
          source,
          destination,
          conflictPolicy,
        }),
      );
      setPanel("transfers");
    } catch (reason) {
      setError(errorMessage(reason));
    }
  };
  const transferFolder = async (direction: "upload" | "download") => {
    if (!api.isDesktop()) {
      setError("文件夹传输需要运行 CNshell 桌面版");
      return;
    }
    let source: string;
    let destination: string;
    if (direction === "upload") {
      const chosen = await open({ multiple: false, directory: true });
      if (!chosen) return;
      source = chosen;
      destination = await api.joinRemotePath(path, localPathName(chosen));
    } else {
      if (!selected || selected.kind !== "directory") return;
      const chosen = await open({ multiple: false, directory: true });
      if (!chosen) return;
      source = selected.path;
      destination = joinLocalPath(
        chosen,
        selected.name,
        platform.operatingSystem,
      );
    }
    const exists =
      direction === "upload"
        ? files.some((item) => item.path === destination)
        : false;
    const conflictPolicy: Exclude<ConflictPolicy, "ask"> =
      exists && confirm("目标文件夹已存在，是否覆盖？选择“取消”将自动重命名。")
        ? "overwrite"
        : "rename";
    await runBackground(
      direction === "upload"
        ? "正在打包并上传文件夹…"
        : "正在打包并下载文件夹…",
      () =>
        api.startDirectoryTransfer(
          session.id,
          direction,
          source,
          destination,
          conflictPolicy,
        ),
    );
  };
  const download = async () => {
    if (!selected || selected.kind !== "file") return;
    const destination = await save({ defaultPath: selected.name });
    if (!destination) return;
    const conflictPolicy = chooseConflictPolicy(
      "如果本地已存在目标文件，请选择冲突策略。",
    );
    try {
      addTransfer(
        await api.enqueueTransfer({
          sessionId: session.id,
          direction: "download",
          source: selected.path,
          destination,
          conflictPolicy,
        }),
      );
      setPanel("transfers");
    } catch (reason) {
      setError(errorMessage(reason));
    }
  };
  const sortBy = (key: SortKey) =>
    setSort((current) =>
      current.key === key ? { key, asc: !current.asc } : { key, asc: true },
    );
  const toggleColumn = (column: FileColumn) =>
    setVisibleColumns((current) => {
      const next = new Set(current);
      if (next.has(column)) next.delete(column);
      else next.add(column);
      return next;
    });
  const runBackground = async (
    label: string,
    start: () => ReturnType<typeof api.startArchiveRemote>,
  ) => {
    if (background) {
      setError("已有文件后台任务正在运行");
      return;
    }
    try {
      const task = await start();
      setBackground({ id: task.id, label });
      await waitForTask(task);
      await load();
    } catch (reason) {
      if ((reason as DOMException).name !== "AbortError")
        setError(errorMessage(reason));
    } finally {
      setBackground(null);
    }
  };
  const resizeColumn = (column: FileTableColumn, width: number) =>
    setColumnWidths((current) => ({
      ...current,
      [column]: clampFileColumnWidth(column, width),
    }));
  const startColumnResize = (
    column: FileTableColumn,
    event: ReactPointerEvent<HTMLSpanElement>,
  ) => {
    event.preventDefault();
    const startX = event.clientX;
    const startWidth = columnWidths[column];
    const pointerId = event.pointerId;
    event.currentTarget.setPointerCapture(pointerId);
    const move = (moveEvent: globalThis.PointerEvent) => {
      if (moveEvent.pointerId === pointerId)
        resizeColumn(column, startWidth + moveEvent.clientX - startX);
    };
    const stop = (upEvent: globalThis.PointerEvent) => {
      if (upEvent.pointerId !== pointerId) return;
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", stop);
      window.removeEventListener("pointercancel", stop);
    };
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", stop);
    window.addEventListener("pointercancel", stop);
  };
  const resizeColumnByKeyboard = (
    column: FileTableColumn,
    event: ReactKeyboardEvent<HTMLSpanElement>,
  ) => {
    if (event.key !== "ArrowLeft" && event.key !== "ArrowRight") return;
    event.preventDefault();
    resizeColumn(
      column,
      columnWidths[column] + (event.key === "ArrowRight" ? 10 : -10),
    );
  };
  const archive = async (extract: boolean) => {
    if (!selected) return;
    await runBackground(extract ? "正在解压…" : "正在压缩…", () =>
      api.startArchiveRemote(session.id, selected.path, extract),
    );
  };
  const openLocally = async (application?: string) => {
    if (!selected) return;
    await runBackground("正在准备本地预览…", () =>
      api.startOpenRemoteLocally(session.id, selected.path, application),
    );
  };
  const openWith = async () => {
    const application = await open(
      externalApplicationDialogOptions(
        platform,
        `选择用于打开文件的${platform.displayName}应用`,
      ),
    );
    if (application) await openLocally(application);
  };
  const uploadPaths = useCallback(
    async (paths: string[]) => {
      const destinations = await Promise.all(
        paths.map(async (source) => ({
          source,
          destination: await api.joinRemotePath(path, localPathName(source)),
        })),
      );
      const hasConflict = destinations.some(({ destination }) =>
        files.some((item) => item.path === destination),
      );
      const policy = hasConflict
        ? chooseConflictPolicy(
            "拖入项目中存在远端同名文件，此选择将应用到本批全部冲突。 ",
          )
        : "ask";
      for (const item of destinations)
        addTransfer(
          await api.enqueueTransfer({
            sessionId: session.id,
            direction: "upload",
            ...item,
            conflictPolicy: policy,
          }),
        );
      setPanel("transfers");
    },
    [chooseConflictPolicy, files, path, session.id, setPanel, addTransfer],
  );
  useEffect(() => {
    uploadPathsRef.current = uploadPaths;
  }, [uploadPaths]);
  useEffect(() => {
    if (!api.isDesktop()) return;
    let disposed = false;
    let unlisten: (() => void) | undefined;
    void getCurrentWebview()
      .onDragDropEvent((event) => {
        const payload = event.payload;
        if (payload.type === "leave") {
          setDragging(false);
          return;
        }
        const dropTarget = browserRef.current;
        const inside = Boolean(
          dropTarget && nativeDropIsInsideElement(payload.position, dropTarget),
        );
        if (payload.type === "enter" || payload.type === "over") {
          setDragging(inside);
          return;
        }
        setDragging(false);
        if (inside && payload.paths.length)
          void uploadPathsRef
            .current(payload.paths)
            .catch((reason) => setError(errorMessage(reason)));
      })
      .then((stop) => {
        if (disposed) stop();
        else unlisten = stop;
      })
      .catch((reason) => {
        if (!disposed)
          setError(`无法启用原生文件拖放：${errorMessage(reason)}`);
      });
    return () => {
      disposed = true;
      setDragging(false);
      unlisten?.();
    };
  }, [session.id, setError]);
  const drop = async (event: React.DragEvent) => {
    event.preventDefault();
    setDragging(false);
    const paths = Array.from(event.dataTransfer.files)
      .map((file) => (file as File & { path?: string }).path)
      .filter((value): value is string => Boolean(value));
    if (!paths.length) {
      setError(
        `${platform.displayName} WebView 未提供拖入文件路径，请使用上传按钮`,
      );
      return;
    }
    try {
      await uploadPaths(paths);
    } catch (error) {
      setError(errorMessage(error));
    }
  };
  const fileStyle = {
    "--file-font-size": `${fileFontSize}px`,
    "--file-columns": fileGridTemplate(visibleColumns, columnWidths),
  } as CSSProperties;
  return (
    <div className="file-manager" style={fileStyle}>
      <div className="file-toolbar">
        <IconButton
          icon={ArrowLeft}
          label="上级目录"
          onClick={parent}
          disabled={path === "/"}
        />
        <div className={`file-path-control${pathEditing ? " is-editing" : ""}`}>
          {pathEditing ? (
            <form
              className="file-path-editor"
              onSubmit={(event) => {
                event.preventDefault();
                void submitEditedPath();
              }}
            >
              <input
                ref={pathInputRef}
                value={draftPath}
                onChange={(event) => {
                  setDraftPath(event.target.value);
                  setPathEditError(null);
                }}
                onKeyDown={(event) => {
                  if (event.key !== "Escape") return;
                  event.preventDefault();
                  event.stopPropagation();
                  cancelPathEditing();
                }}
                aria-label="远程路径"
                aria-invalid={Boolean(pathEditError)}
                aria-describedby={
                  pathEditError ? "remote-path-edit-error" : undefined
                }
                disabled={pathSubmitting}
              />
              <IconButton
                icon={Check}
                label="前往输入路径"
                onClick={() => void submitEditedPath()}
                disabled={pathSubmitting}
              />
              <IconButton
                icon={X}
                label="取消编辑路径"
                onClick={cancelPathEditing}
                disabled={pathSubmitting}
              />
            </form>
          ) : (
            <>
              <IconButton
                icon={homeLoading ? LoaderCircle : House}
                label="用户主目录"
                className={homeLoading ? "file-path-home-loading" : ""}
                onClick={() => void goHome()}
                disabled={homeLoading}
              />
              <nav className="file-breadcrumbs" aria-label="当前远程路径">
                {compactRemoteBreadcrumbs(path).map((item, index, items) => {
                  const current = index === items.length - 1;
                  return (
                    <span key={`${item.path}:${item.label}`}>
                      {index > 0 && (
                        <ChevronRight size={12} aria-hidden="true" />
                      )}
                      <button
                        onClick={() =>
                          item.collapsed || current
                            ? beginPathEditing()
                            : navigate(item.path)
                        }
                        aria-current={current ? "page" : undefined}
                        aria-label={index === 0 ? "根目录 /" : item.label}
                        title={
                          item.collapsed
                            ? `完整路径：${path}`
                            : current
                              ? `编辑路径 ${path}`
                              : item.path
                        }
                      >
                        {index === 0 && (
                          <FolderRoot size={13} aria-hidden="true" />
                        )}
                        <span>{item.label}</span>
                      </button>
                    </span>
                  );
                })}
              </nav>
              <IconButton
                icon={Pencil}
                label="编辑远程路径"
                onClick={beginPathEditing}
              />
            </>
          )}
          {pathEditError && (
            <div
              className="file-path-edit-error"
              id="remote-path-edit-error"
              role="alert"
            >
              {pathEditError}
            </div>
          )}
        </div>
        <IconButton
          icon={RefreshCw}
          label="刷新"
          onClick={() => {
            void load();
            refreshTree();
          }}
        />
        <span className="toolbar-separator" />
        <IconButton
          icon={FolderPlus}
          label="新建文件夹"
          onClick={() => void createFolder()}
        />
        <IconButton
          icon={ArrowUpToLine}
          label="上传文件"
          onClick={() => void upload()}
        />
        <IconButton
          icon={FolderPlus}
          label="上传文件夹"
          onClick={() => void transferFolder("upload")}
        />
        <IconButton
          icon={ArrowDownToLine}
          label={selected?.kind === "directory" ? "下载文件夹" : "下载文件"}
          onClick={() =>
            selected?.kind === "directory"
              ? void transferFolder("download")
              : void download()
          }
          disabled={!selected || !["file", "directory"].includes(selected.kind)}
        />
        <span className="toolbar-separator" />
        <PanelFontSizeControl
          value={fileFontSize}
          onChange={setFileFontSize}
          onAutomatic={useAutomaticFileFontSize}
          automatic={fileFontSizeAutomatic}
          label="文件区"
        />
        <div className="file-actions" ref={actionsRef}>
          <IconButton
            icon={MoreHorizontal}
            label="更多文件操作"
            disabled={!selected || Boolean(background)}
            active={actionsOpen}
            onClick={() => setActionsOpen((open) => !open)}
          />
          {selected && actionsOpen && (
            <div
              className="file-action-menu"
              onClickCapture={() => setActionsOpen(false)}
            >
              <button
                onClick={() =>
                  selected.kind === "file" && setEditor(selected.path)
                }
              >
                <FileCode2 size={14} />
                编辑文本
              </button>
              {selected.kind === "file" && (
                <>
                  <button onClick={() => void openLocally()}>
                    <File size={14} />
                    使用默认应用打开
                  </button>
                  <button onClick={() => void openWith()}>
                    <FileCode2 size={14} />
                    选择应用打开…
                  </button>
                </>
              )}
              <button
                onClick={() =>
                  void navigator.clipboard.writeText(selected.path)
                }
              >
                <Clipboard size={14} />
                复制路径
              </button>
              <button onClick={() => void archive(false)}>
                <Archive size={14} />
                压缩为 tar.gz
              </button>
              {selected.name.endsWith(".tar.gz") && (
                <button onClick={() => void archive(true)}>
                  <PackageOpen size={14} />
                  解压到当前目录
                </button>
              )}
              <button onClick={rename}>重命名</button>
              <button onClick={chmod}>修改权限</button>
              <button className="danger" onClick={remove}>
                <Trash2 size={14} />
                删除
              </button>
            </div>
          )}
        </div>
      </div>
      <div className="file-filter-bar">
        <IconButton
          icon={treeOpen ? PanelLeftClose : PanelLeftOpen}
          label={treeOpen ? "折叠目录树" : "展开目录树"}
          active={treeOpen}
          onClick={() => setTreeOpen((open) => !open)}
        />
        <label>
          <Search size={14} />
          <input
            value={fileQuery}
            onChange={(event) => setFileQuery(event.target.value)}
            placeholder="筛选当前目录"
            aria-label="筛选当前目录文件"
          />
          <span>
            {sorted.length}/{files.length}
          </span>
        </label>
        <details className="file-column-picker">
          <summary aria-label="选择显示列" title="选择显示列">
            <Columns3 size={15} />
          </summary>
          <div>
            {fileColumnOptions.map((option) => (
              <label key={option.value}>
                <input
                  type="checkbox"
                  checked={visibleColumns.has(option.value)}
                  onChange={() => toggleColumn(option.value)}
                />
                <span>{option.label}</span>
              </label>
            ))}
          </div>
        </details>
      </div>
      <div
        ref={browserRef}
        className={`file-browser ${treeOpen ? "" : "tree-hidden"} ${dragging ? "dragging" : ""}`}
        onDragEnter={(event) => {
          event.preventDefault();
          setDragging(true);
        }}
        onDragOver={(event) => event.preventDefault()}
        onDragLeave={(event) => {
          if (!event.currentTarget.contains(event.relatedTarget as Node | null))
            setDragging(false);
        }}
        onDrop={(event) => void drop(event)}
      >
        {treeOpen && (
          <RemoteDirectoryTree
            key={`${session.id}-${settings.showHiddenFiles}`}
            activePath={path}
            initialExpanded={restored?.expandedPaths}
            listDirectories={listTreeDirectories}
            onNavigate={navigate}
            onError={reportTreeError}
            onExpandedChange={rememberExpanded}
          />
        )}
        <div
          className="file-table"
          role="table"
          aria-label={`远程目录 ${path}`}
          aria-rowcount={sorted.length + 1}
          aria-colcount={visibleColumns.size + 1}
          aria-busy={loading}
        >
          <div className="file-head" role="row" aria-rowindex={1}>
            <FileColumnHeader
              column="name"
              label="名称"
              width={columnWidths.name}
              sort={sort}
              sortKey="name"
              onSort={sortBy}
              onPointerDown={startColumnResize}
              onKeyDown={resizeColumnByKeyboard}
            />
            {visibleColumns.has("size") && (
              <FileColumnHeader
                column="size"
                label="大小"
                width={columnWidths.size}
                sort={sort}
                sortKey="size"
                onSort={sortBy}
                onPointerDown={startColumnResize}
                onKeyDown={resizeColumnByKeyboard}
              />
            )}
            {visibleColumns.has("kind") && (
              <FileColumnHeader
                column="kind"
                label="类型"
                width={columnWidths.kind}
                sort={sort}
                onSort={sortBy}
                onPointerDown={startColumnResize}
                onKeyDown={resizeColumnByKeyboard}
              />
            )}
            {visibleColumns.has("modifiedAt") && (
              <FileColumnHeader
                column="modifiedAt"
                label="修改时间"
                width={columnWidths.modifiedAt}
                sort={sort}
                sortKey="modifiedAt"
                onSort={sortBy}
                onPointerDown={startColumnResize}
                onKeyDown={resizeColumnByKeyboard}
              />
            )}
            {visibleColumns.has("permissions") && (
              <FileColumnHeader
                column="permissions"
                label="权限"
                width={columnWidths.permissions}
                sort={sort}
                onSort={sortBy}
                onPointerDown={startColumnResize}
                onKeyDown={resizeColumnByKeyboard}
              />
            )}
            {visibleColumns.has("owner") && (
              <FileColumnHeader
                column="owner"
                label="用户/组"
                width={columnWidths.owner}
                sort={sort}
                onSort={sortBy}
                onPointerDown={startColumnResize}
                onKeyDown={resizeColumnByKeyboard}
              />
            )}
          </div>
          <div
            className="file-body"
            ref={bodyRef}
            onScroll={(event) => setScrollTop(event.currentTarget.scrollTop)}
          >
            {loading ? (
              <div className="loading-state" role="status">
                <LoaderCircle className="spin" />
                读取目录…
              </div>
            ) : directoryError ? (
              <div className="file-load-error" role="alert">
                <AlertCircle size={25} />
                <strong>无法读取目录</strong>
                <span>{directoryError}</span>
                <button
                  className="button secondary"
                  aria-label="重试读取目录"
                  onClick={() => void load()}
                >
                  <RotateCcw size={13} />
                  重试
                </button>
              </div>
            ) : (
              <>
                <div aria-hidden="true" style={{ height: windowRange.top }} />
                {visibleFiles.map((item, index) => (
                  <button
                    key={item.path}
                    className={`file-row ${selected?.path === item.path ? "selected" : ""}`}
                    onClick={() => setSelected(item)}
                    onContextMenu={(event) => {
                      event.preventDefault();
                      setSelected(item);
                      setContextMenu({ x: event.clientX, y: event.clientY });
                    }}
                    onDoubleClick={() =>
                      item.kind === "directory"
                        ? navigate(item.path)
                        : setEditor(item.path)
                    }
                    role="row"
                    aria-selected={selected?.path === item.path}
                    aria-rowindex={windowRange.start + index + 2}
                  >
                    <span role="cell">
                      <>
                        {item.kind === "directory" ? (
                          <Folder size={15} aria-hidden="true" />
                        ) : (
                          <File size={15} aria-hidden="true" />
                        )}
                      </>
                      <strong>{item.name}</strong>
                    </span>
                    {visibleColumns.has("size") && (
                      <span role="cell">
                        {item.kind === "directory"
                          ? "—"
                          : formatBytes(item.size)}
                      </span>
                    )}
                    {visibleColumns.has("kind") && (
                      <span role="cell">{remoteFileKindLabel(item.kind)}</span>
                    )}
                    {visibleColumns.has("modifiedAt") && (
                      <span role="cell">
                        {item.modifiedAt
                          ? new Date(item.modifiedAt * 1000).toLocaleString()
                          : "—"}
                      </span>
                    )}
                    {visibleColumns.has("permissions") && (
                      <code role="cell">{item.permissions}</code>
                    )}
                    {visibleColumns.has("owner") && (
                      <span role="cell">
                        {item.owner ?? "—"}/{item.group ?? "—"}
                      </span>
                    )}
                  </button>
                ))}
                <div
                  aria-hidden="true"
                  style={{ height: windowRange.bottom }}
                />
              </>
            )}
            {!loading && !directoryError && !sorted.length && (
              <div className="empty-files">
                <Folder size={28} />
                <span>
                  {fileQuery
                    ? "当前筛选下没有文件"
                    : api.isDesktop()
                      ? "此目录为空"
                      : "连接真实 SSH 会话后浏览远端文件"}
                </span>
              </div>
            )}
          </div>
        </div>
        {dragging && (
          <div className="drop-overlay" role="status" aria-live="polite">
            <ArrowUpToLine size={28} />
            <strong>拖放上传到 {path}</strong>
            <span>同名文件自动重命名</span>
          </div>
        )}
      </div>
      {contextMenu && selected && (
        <div
          className="file-context-menu"
          role="menu"
          aria-label={`${selected.name} 文件操作`}
          style={{ left: contextMenu.x, top: contextMenu.y }}
          onPointerDown={(event) => event.stopPropagation()}
        >
          <button
            role="menuitem"
            onClick={() => selected.kind === "file" && setEditor(selected.path)}
            disabled={selected.kind !== "file"}
          >
            <FileCode2 size={14} />
            编辑文本
          </button>
          {selected.kind === "file" && (
            <>
              <button role="menuitem" onClick={() => void openLocally()}>
                <File size={14} />
                使用默认应用打开
              </button>
              <button role="menuitem" onClick={() => void openWith()}>
                <FileCode2 size={14} />
                选择应用打开…
              </button>
            </>
          )}
          <button
            role="menuitem"
            onClick={() =>
              selected.kind === "directory"
                ? void transferFolder("download")
                : void download()
            }
            disabled={!["file", "directory"].includes(selected.kind)}
          >
            <ArrowDownToLine size={14} />
            下载
          </button>
          <button
            role="menuitem"
            onClick={() =>
              void upload(selected.kind === "directory" ? selected.path : path)
            }
          >
            <ArrowUpToLine size={14} />
            上传文件到此处
          </button>
          <button
            role="menuitem"
            onClick={() =>
              void createFile(
                selected.kind === "directory" ? selected.path : path,
              )
            }
          >
            <FilePlus size={14} />
            新建文件
          </button>
          <button
            role="menuitem"
            onClick={() =>
              void createFolder(
                selected.kind === "directory" ? selected.path : path,
              )
            }
          >
            <FolderPlus size={14} />
            新建文件夹
          </button>
          <button
            role="menuitem"
            onClick={() => void navigator.clipboard.writeText(selected.path)}
          >
            <Clipboard size={14} />
            复制路径
          </button>
          <button role="menuitem" onClick={() => void archive(false)}>
            <Archive size={14} />
            压缩为 tar.gz
          </button>
          {selected.name.endsWith(".tar.gz") && (
            <button role="menuitem" onClick={() => void archive(true)}>
              <PackageOpen size={14} />
              解压到当前目录
            </button>
          )}
          <button role="menuitem" onClick={() => void rename()}>
            重命名
          </button>
          <button role="menuitem" onClick={() => void chmod()}>
            修改权限
          </button>
          <button
            role="menuitem"
            className="danger"
            onClick={() => void remove()}
          >
            <Trash2 size={14} />
            删除
          </button>
        </div>
      )}
      <footer className="file-status">
        <span>
          {background ? (
            <>
              <LoaderCircle className="spin" size={12} />
              {background.label}
              <button onClick={() => void api.cancelTask(background.id)}>
                取消
              </button>
            </>
          ) : (
            `${files.length} 个项目`
          )}
        </span>
        <span>
          {selected
            ? selected.path
            : api.isDesktop()
              ? "SFTP 已就绪"
              : "等待桌面连接"}
        </span>
      </footer>
    </div>
  );
}

function FileColumnHeader({
  column,
  label,
  width,
  sort,
  sortKey,
  onSort,
  onPointerDown,
  onKeyDown,
}: {
  column: FileTableColumn;
  label: string;
  width: number;
  sort: { key: SortKey; asc: boolean };
  sortKey?: SortKey;
  onSort: (key: SortKey) => void;
  onPointerDown: (
    column: FileTableColumn,
    event: ReactPointerEvent<HTMLSpanElement>,
  ) => void;
  onKeyDown: (
    column: FileTableColumn,
    event: ReactKeyboardEvent<HTMLSpanElement>,
  ) => void;
}) {
  const ariaSort = sortKey
    ? sort.key === sortKey
      ? sort.asc
        ? "ascending"
        : "descending"
      : "none"
    : undefined;
  return (
    <div
      className="file-column-header"
      role="columnheader"
      aria-label={label}
      aria-sort={ariaSort}
    >
      {sortKey ? (
        <button type="button" onClick={() => onSort(sortKey)}>
          {label}
          {sort.key === sortKey && (
            <ChevronDown
              size={12}
              aria-hidden="true"
              className={sort.asc ? "" : "descending"}
            />
          )}
        </button>
      ) : (
        <span>{label}</span>
      )}
      <span
        className="file-column-resizer"
        role="separator"
        aria-label={`调整${label}列宽`}
        aria-orientation="vertical"
        aria-valuemin={fileColumnWidthBounds[column][0]}
        aria-valuemax={fileColumnWidthBounds[column][1]}
        aria-valuenow={width}
        tabIndex={0}
        onPointerDown={(event) => onPointerDown(column, event)}
        onKeyDown={(event) => onKeyDown(column, event)}
      />
    </div>
  );
}

const remoteFileKindLabel = (kind: RemoteFile["kind"]) =>
  ({ file: "文件", directory: "文件夹", symlink: "符号链接", other: "其他" })[
    kind
  ] ?? kind;

const fileColumnOptions: { value: FileColumn; label: string }[] = [
  { value: "size", label: "大小" },
  { value: "kind", label: "类型" },
  { value: "modifiedAt", label: "修改时间" },
  { value: "permissions", label: "权限" },
  { value: "owner", label: "用户/组" },
];

function readFileColumns(): FileColumn[] {
  try {
    const value: unknown = JSON.parse(
      localStorage.getItem("cnshell-file-columns") ?? "null",
    );
    if (!Array.isArray(value)) return defaultFileColumns;
    const allowed = new Set<FileColumn>(defaultFileColumns);
    return value.filter(
      (item): item is FileColumn =>
        typeof item === "string" && allowed.has(item as FileColumn),
    );
  } catch {
    return defaultFileColumns;
  }
}

const defaultFileColumnWidths: FileColumnWidths = {
  name: 190,
  size: 75,
  kind: 65,
  modifiedAt: 140,
  permissions: 90,
  owner: 75,
};

const fileColumnWidthBounds: Record<FileTableColumn, [number, number]> = {
  name: [140, 520],
  size: [58, 160],
  kind: [58, 160],
  modifiedAt: [110, 260],
  permissions: [72, 180],
  owner: [68, 220],
};

function clampFileColumnWidth(column: FileTableColumn, width: number) {
  const [minimum, maximum] = fileColumnWidthBounds[column];
  return Math.min(maximum, Math.max(minimum, Math.round(width)));
}

function readFileColumnWidths(): FileColumnWidths {
  try {
    const stored = JSON.parse(
      localStorage.getItem("cnshell-file-column-widths") ?? "null",
    ) as Partial<Record<FileTableColumn, unknown>> | null;
    if (!stored || typeof stored !== "object") return defaultFileColumnWidths;
    return Object.fromEntries(
      (Object.keys(defaultFileColumnWidths) as FileTableColumn[]).map(
        (column) => {
          const width = stored[column];
          return [
            column,
            typeof width === "number" && Number.isFinite(width)
              ? clampFileColumnWidth(column, width)
              : defaultFileColumnWidths[column],
          ];
        },
      ),
    ) as FileColumnWidths;
  } catch {
    return defaultFileColumnWidths;
  }
}

function fileGridTemplate(columns: Set<FileColumn>, widths: FileColumnWidths) {
  return [
    `minmax(${widths.name}px, 2fr)`,
    ...defaultFileColumns
      .filter((column) => columns.has(column))
      .map((column) => `${widths[column]}px`),
  ].join(" ");
}

function remoteBreadcrumbs(path: string): RemoteBreadcrumb[] {
  if (path.startsWith("cnshell-raw-path:"))
    return [{ label: "原始路径", path }];
  const parts = path.split("/").filter(Boolean);
  const breadcrumbs = [{ label: "/", path: "/" }];
  let current = "";
  for (const part of parts) {
    current += `/${part}`;
    breadcrumbs.push({ label: part, path: current });
  }
  return breadcrumbs;
}

function compactRemoteBreadcrumbs(path: string): RemoteBreadcrumb[] {
  const breadcrumbs = remoteBreadcrumbs(path);
  if (breadcrumbs.length <= 5) return breadcrumbs;
  return [
    breadcrumbs[0],
    {
      label: "…",
      path: breadcrumbs.at(-3)?.path ?? "/",
      collapsed: true,
    },
    ...breadcrumbs.slice(-2),
  ];
}

const normalizeRemotePath = (value: string) => {
  const parts = value.split("/").filter((part) => part && part !== ".");
  const normalized: string[] = [];
  for (const part of parts) {
    if (part === "..") normalized.pop();
    else normalized.push(part);
  }
  return `/${normalized.join("/")}`;
};
