import { render,screen,waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach,describe,expect,it,vi } from "vitest";
import { api } from "../../lib/api";
import { TextEditor } from "./TextEditor";

describe("TextEditor",()=>{
  beforeEach(()=>{vi.restoreAllMocks();vi.spyOn(api,"openText").mockResolvedValue({content:'{"name":"CNshell"}',modifiedAt:10});vi.spyOn(api,"saveText").mockResolvedValue(undefined);});

  it("formats JSON and preserves optimistic atomic-save metadata",async()=>{const user=userEvent.setup();render(<TextEditor sessionId="session-1" path="/etc/app.json" onClose={vi.fn()}/>);expect(await screen.findByLabelText("远程文本内容")).toBeInTheDocument();await user.click(screen.getByRole("button",{name:"格式化"}));await user.click(screen.getByRole("button",{name:"原子保存"}));await waitFor(()=>expect(api.saveText).toHaveBeenCalledWith("session-1","/etc/app.json",'{\n  "name": "CNshell"\n}\n',10));});

  it("shows base, local and remote versions after a save conflict",async()=>{vi.spyOn(api,"saveText").mockRejectedValue({message:"远端文件已被其他程序修改，请重新加载后合并"});vi.mocked(api.openText).mockResolvedValueOnce({content:'{"value":1}',modifiedAt:10}).mockResolvedValueOnce({content:'{"value":2}',modifiedAt:11});const user=userEvent.setup();render(<TextEditor sessionId="session-1" path="/etc/app.json" onClose={vi.fn()}/>);await screen.findByLabelText("远程文本内容");await user.click(screen.getByRole("button",{name:"格式化"}));await user.click(screen.getByRole("button",{name:"原子保存"}));expect(await screen.findByText("远端文件在编辑期间发生变化")).toBeInTheDocument();expect(screen.getByRole("heading",{name:"基础版本"})).toBeInTheDocument();expect(screen.getByRole("heading",{name:"本地版本"})).toBeInTheDocument();expect(screen.getByRole("heading",{name:"远端版本"})).toBeInTheDocument();expect(screen.getByText(/"value":2/)).toBeInTheDocument();await user.click(screen.getByRole("button",{name:"使用远端版本"}));expect(screen.queryByText("远端文件在编辑期间发生变化")).not.toBeInTheDocument();});

  it("imports an isolated external edit through the same atomic save check",async()=>{vi.spyOn(api,"startExternalEdit").mockResolvedValue({id:"edit-1",remotePath:"/etc/app.json",localPath:"/tmp/CNshellExternalEdit/edit-1/app.json",expectedModifiedAt:10,startedAt:""});vi.spyOn(api,"readExternalEdit").mockResolvedValue({id:"edit-1",content:'{"name":"external"}',expectedModifiedAt:10});const discard=vi.spyOn(api,"discardExternalEdit").mockResolvedValue(undefined);const user=userEvent.setup();render(<TextEditor sessionId="session-1" path="/etc/app.json" onClose={vi.fn()}/>);await screen.findByLabelText("远程文本内容");await user.click(screen.getByRole("button",{name:"外部应用"}));expect(await screen.findByText("外部编辑副本已打开")).toBeInTheDocument();await user.click(screen.getByRole("button",{name:"读取并回传"}));await waitFor(()=>expect(api.saveText).toHaveBeenCalledWith("session-1","/etc/app.json",'{"name":"external"}',10));expect(discard).toHaveBeenCalledWith("edit-1");});
});
