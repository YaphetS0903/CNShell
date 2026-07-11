import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { ConnectionFolderTree } from "./ConnectionFolderTree";

describe("ConnectionFolderTree",()=>{
  it("expands nested folders and selects children",async()=>{
    const onSelect=vi.fn();const onToggle=vi.fn();
    const folders=[{id:"root",name:"生产",parentId:null,sortOrder:0},{id:"child",name:"华南",parentId:"root",sortOrder:0}];
    const props={folders,activeId:"",expanded:new Set<string>(),counts:new Map([["child",2]]),onSelect,onToggle,onRename:vi.fn(),onDelete:vi.fn(),onDropConnection:vi.fn()};
    const view=render(<ConnectionFolderTree {...props}/>);
    await userEvent.click(screen.getByRole("button",{name:"展开 生产"}));expect(onToggle).toHaveBeenCalledWith("root");
    view.rerender(<ConnectionFolderTree {...props} activeId="child" expanded={new Set(["root"])}/>);
    await userEvent.click(screen.getByRole("button",{name:/^华南 2$/}));expect(onSelect).toHaveBeenCalledWith("child");expect(screen.getAllByRole("treeitem").at(-1)).toHaveAttribute("aria-selected","true");
  });
});
