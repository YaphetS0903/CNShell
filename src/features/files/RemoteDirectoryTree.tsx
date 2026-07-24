import { ChevronRight, Folder, LoaderCircle, RotateCcw } from "lucide-react";
import { useCallback, useEffect, useRef, useState } from "react";
import { DIRECTORY_REQUEST_TIMEOUT_MS, withTimeout } from "../../lib/async-timeout";

type DirectoryNode = { name: string; path: string };

type RemoteDirectoryTreeProps = {
  activePath: string;
  initialExpanded?: string[];
  listDirectories: (path: string) => Promise<DirectoryNode[]>;
  onNavigate: (path: string) => void;
  onError: (reason: unknown) => void;
  onExpandedChange?: (paths: string[]) => void;
};

export function RemoteDirectoryTree({ activePath, initialExpanded, listDirectories, onNavigate, onError, onExpandedChange }: RemoteDirectoryTreeProps) {
  const [expanded, setExpanded] = useState(() => new Set(["/", ...(initialExpanded ?? [])]));
  const [children, setChildren] = useState<Record<string, DirectoryNode[]>>({});
  const [loading, setLoading] = useState(() => new Set<string>());
  const [loadErrors, setLoadErrors] = useState(() => new Set<string>());
  const loadedPaths = useRef(new Set<string>());
  const loadingPaths = useRef(new Set<string>());

  useEffect(() => {
    onExpandedChange?.([...expanded]);
  }, [expanded, onExpandedChange]);

  const loadChildren = useCallback(async (path: string, force = false) => {
    if (loadingPaths.current.has(path) || (!force && loadedPaths.current.has(path))) return;
    loadingPaths.current.add(path);
    setLoading((current) => new Set(current).add(path));
    setLoadErrors((current) => {
      const next = new Set(current);
      next.delete(path);
      return next;
    });
    try {
      const result = await withTimeout(
        listDirectories(path),
        DIRECTORY_REQUEST_TIMEOUT_MS,
        `目录树读取 ${path} 超时，请重试`,
      );
      loadedPaths.current.add(path);
      setChildren((current) => ({ ...current, [path]: result }));
    } catch (reason) {
      onError(reason);
      setLoadErrors((current) => {
        const next = new Set(current);
        next.add(path);
        return next;
      });
    } finally {
      loadingPaths.current.delete(path);
      setLoading((current) => {
        const next = new Set(current);
        next.delete(path);
        return next;
      });
    }
  }, [listDirectories, onError]);

  useEffect(() => {
    void loadChildren("/");
    for (const path of initialExpanded ?? []) {
      if (path !== "/") void loadChildren(path);
    }
  }, [initialExpanded, loadChildren]);

  useEffect(() => {
    const segments = activePath.split("/").filter(Boolean);
    const ancestors = ["/", ...segments.slice(0, -1).map((_, index) => `/${segments.slice(0, index + 1).join("/")}`)];
    setExpanded((current) => new Set([...current, ...ancestors]));
    for (const path of ancestors) void loadChildren(path);
  }, [activePath, loadChildren]);

  useEffect(() => {
    const refresh = () => {
      for (const path of expanded) void loadChildren(path, true);
    };
    window.addEventListener("cnshell-refresh-directory-tree", refresh);
    return () => window.removeEventListener("cnshell-refresh-directory-tree", refresh);
  }, [expanded, loadChildren]);

  const toggle = (path: string) => {
    if (loadErrors.has(path)) {
      setExpanded((current) => new Set(current).add(path));
      void loadChildren(path, true);
      return;
    }
    if (expanded.has(path)) {
      setExpanded((current) => {
        const next = new Set(current);
        next.delete(path);
        return next;
      });
      return;
    }
    setExpanded((current) => new Set(current).add(path));
    if (!children[path]) void loadChildren(path);
  };

  const open = (path: string) => {
    onNavigate(path);
    if (!expanded.has(path)) toggle(path);
  };

  const renderNode = (node: DirectoryNode, depth: number) => {
    const isExpanded = expanded.has(node.path);
    const isLoading = loading.has(node.path);
    const loadFailed = loadErrors.has(node.path);
    return <div key={node.path} role="treeitem" aria-expanded={isExpanded} aria-selected={activePath === node.path}>
      <div className={`remote-tree-row ${activePath === node.path ? "active" : ""}`} style={{ paddingLeft: `${5 + depth * 12}px` }}>
        <button className={`remote-tree-toggle ${loadFailed ? "failed" : ""}`} aria-label={loadFailed ? `重试加载 ${node.name}` : `${isExpanded ? "折叠" : "展开"} ${node.name}`} title={loadFailed ? "目录读取失败，点击重试" : undefined} onClick={() => toggle(node.path)}>
          {isLoading ? <LoaderCircle className="spin" size={12} /> : loadFailed ? <RotateCcw size={12} /> : <ChevronRight className={isExpanded ? "expanded" : ""} size={13} />}
        </button>
        <button className="remote-tree-name" onClick={() => open(node.path)} title={node.path}>
          <Folder size={13} aria-hidden="true" />
          <span>{node.name}</span>
        </button>
      </div>
      {isExpanded && children[node.path]?.length ? <div role="group">{children[node.path].map((child) => renderNode(child, depth + 1))}</div> : null}
    </div>;
  };

  return <nav className="remote-tree" aria-label="远端目录树">
    <div role="tree">{renderNode({ name: "/", path: "/" }, 0)}</div>
  </nav>;
}
