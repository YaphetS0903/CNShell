import { ChevronRight, Folder, MoreHorizontal } from "lucide-react";
import type { Folder as FolderModel } from "../../types";

type Props = {
  folders: FolderModel[]; activeId: string; expanded: Set<string>; counts: Map<string, number>;
  onSelect: (id: string) => void; onToggle: (id: string) => void;
  onRename: (folder: FolderModel) => void; onDelete: (folder: FolderModel) => void;
  onDropConnection: (event: React.DragEvent, folderId: string) => void;
};

export function ConnectionFolderTree({folders,activeId,expanded,counts,onSelect,onToggle,onRename,onDelete,onDropConnection}:Props){
  const children=new Map<string|null,FolderModel[]>();
  for(const folder of folders){const parent=folder.parentId&&folders.some((item)=>item.id===folder.parentId)?folder.parentId:null;children.set(parent,[...(children.get(parent)??[]),folder]);}
  for(const entries of children.values())entries.sort((left,right)=>left.sortOrder-right.sortOrder||left.name.localeCompare(right.name,"zh-CN"));
  const render=(folder:FolderModel,depth:number)=>{const nested=children.get(folder.id)??[];const isExpanded=expanded.has(folder.id);return <div key={folder.id} role="treeitem" aria-expanded={nested.length?isExpanded:undefined} aria-selected={activeId===folder.id}>
    <div className="folder-item" style={{paddingLeft:`${depth*12}px`}} onDragOver={(event)=>event.preventDefault()} onDrop={(event)=>onDropConnection(event,folder.id)}>
      <button className="folder-disclosure" aria-label={nested.length?`${isExpanded?"折叠":"展开"} ${folder.name}`:`${folder.name} 没有子文件夹`} disabled={!nested.length} onClick={()=>onToggle(folder.id)}><ChevronRight className={isExpanded?"expanded":""} size={12}/></button>
      <button className={activeId===folder.id?"active":""} onClick={()=>onSelect(folder.id)}><Folder size={13}/>{folder.name}<span>{counts.get(folder.id)??0}</span></button>
      <button className="folder-more" onClick={()=>onRename(folder)} onContextMenu={(event)=>{event.preventDefault();onDelete(folder);}} aria-label={`${folder.name} 重命名；右键删除`}><MoreHorizontal size={12}/></button>
    </div>
    {nested.length&&isExpanded?<div role="group">{nested.map((child)=>render(child,depth+1))}</div>:null}
  </div>;};
  return <div role="tree" aria-label="连接文件夹树">{(children.get(null)??[]).map((folder)=>render(folder,0))}</div>;
}
