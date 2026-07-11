import { ChevronRight, Folder, LoaderCircle } from "lucide-react";
import { useCallback, useEffect, useState } from "react";

type DirectoryNode = { name: string; path: string };

type RemoteDirectoryTreeProps = {
  activePath: string;
  listDirectories: (path: string) => Promise<DirectoryNode[]>;
  onNavigate: (path: string) => void;
  onError: (reason: unknown) => void;
};

export function RemoteDirectoryTree({ activePath, listDirectories, onNavigate, onError }: RemoteDirectoryTreeProps) {
  const [expanded, setExpanded] = useState(() => new Set(["/"]));
  const [children, setChildren] = useState<Record<string, DirectoryNode[]>>({});
  const [loading, setLoading] = useState(() => new Set<string>());

  const loadChildren = useCallback(async (path: string) => {
    setLoading((current) => new Set(current).add(path));
    try {
      const result = await listDirectories(path);
      setChildren((current) => ({ ...current, [path]: result }));
    } catch (reason) {
      onError(reason);
      setExpanded((current) => {
        const next = new Set(current);
        next.delete(path);
        return next;
      });
    } finally {
      setLoading((current) => {
        const next = new Set(current);
        next.delete(path);
        return next;
      });
    }
  }, [listDirectories, onError]);

  useEffect(() => {
    void loadChildren("/");
  }, [loadChildren]);

  useEffect(() => {
    const refresh = () => {
      for (const path of expanded) void loadChildren(path);
    };
    window.addEventListener("cnshell-refresh-directory-tree", refresh);
    return () => window.removeEventListener("cnshell-refresh-directory-tree", refresh);
  }, [expanded, loadChildren]);

  const toggle = (path: string) => {
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

  const renderNode = (node: DirectoryNode, depth: number) => {
    const isExpanded = expanded.has(node.path);
    const isLoading = loading.has(node.path);
    return <div key={node.path} role="treeitem" aria-expanded={isExpanded} aria-selected={activePath === node.path}>
      <div className={`remote-tree-row ${activePath === node.path ? "active" : ""}`} style={{ paddingLeft: `${5 + depth * 12}px` }}>
        <button className="remote-tree-toggle" aria-label={`${isExpanded ? "折叠" : "展开"} ${node.name}`} onClick={() => toggle(node.path)}>
          {isLoading ? <LoaderCircle className="spin" size={12} /> : <ChevronRight className={isExpanded ? "expanded" : ""} size={13} />}
        </button>
        <button className="remote-tree-name" onClick={() => onNavigate(node.path)} title={node.path}>
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
