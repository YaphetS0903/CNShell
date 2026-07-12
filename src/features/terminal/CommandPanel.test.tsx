import { render,screen,waitFor,within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach,describe,expect,it,vi } from "vitest";
import { api } from "../../lib/api";
import type { TerminalSession } from "../../types";
import { CommandPanel } from "./CommandPanel";

const session:TerminalSession={id:"session-1",connectionId:"connection-1",sessionType:"terminal",title:"测试",status:"online",startedAt:"",lastError:null};

describe("CommandPanel",()=>{
  beforeEach(()=>{vi.restoreAllMocks();vi.spyOn(api,"listSnippets").mockResolvedValue([]);vi.spyOn(api,"listHistory").mockResolvedValue([]);vi.spyOn(api,"addHistory").mockResolvedValue();vi.spyOn(api,"terminalInput").mockResolvedValue();});

  it("collects template values and previews an injection-safe command",async()=>{
    vi.spyOn(api,"listSnippets").mockResolvedValue([{id:"restart",name:"重启服务",command:"systemctl restart {{service}}",description:"",tags:[],sortOrder:0}]);
    const user=userEvent.setup();render(<CommandPanel session={session} onError={vi.fn()}/>);await user.click(await screen.findByRole("button",{name:/^重启服务/}));
    const parameter=screen.getByRole("textbox",{name:"命令参数 service"});await user.type(parameter,"nginx; rm -rf /");
    expect(screen.getByText("systemctl restart 'nginx; rm -rf /'",{selector:"code"})).toBeInTheDocument();await user.click(within(screen.getByRole("dialog",{name:"填写命令参数"})).getByRole("button",{name:"执行"}));
    await waitFor(()=>expect(api.terminalInput).toHaveBeenCalledWith(session.id,"systemctl restart 'nginx; rm -rf /'\n"));
  });

  it("requires explicit confirmation before a high-risk command",async()=>{
    const confirmMock=vi.spyOn(window,"confirm").mockReturnValue(false);const user=userEvent.setup();render(<CommandPanel session={session} onError={vi.fn()}/>);const input=screen.getByRole("combobox",{name:"智能命令输入"});
    await user.type(input,"rm -rf /{Enter}");expect(confirmMock).toHaveBeenCalled();expect(api.terminalInput).not.toHaveBeenCalled();
    confirmMock.mockReturnValue(true);await user.type(input,"{Enter}");await waitFor(()=>expect(api.terminalInput).toHaveBeenCalledWith(session.id,"rm -rf /\n"));
  });
});
