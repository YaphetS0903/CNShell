import { render,screen,waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { useState } from "react";
import { beforeEach,describe,expect,it,vi } from "vitest";
import { api } from "../../lib/api";
import { workspaceRuntime } from "../../lib/workspace-runtime";
import type { CommandSnippet,TerminalSession } from "../../types";
import { SmartCommandEntry } from "./SmartCommandEntry";

const session:TerminalSession={id:"session-1",connectionId:"connection-1",sessionType:"terminal",title:"测试",status:"online",startedAt:"",lastError:null};
const snippets:CommandSnippet[]=[{id:"logs",name:"查看服务日志",command:"journalctl -u nginx",description:"",tags:[],sortOrder:0}];

function Harness({history=["systemctl status nginx"],onRun=vi.fn()}:{history?:string[];onRun?:(command:string)=>void}){
  const[draft,setDraft]=useState("");
  return <SmartCommandEntry session={session} snippets={snippets} history={history} draft={draft} setDraft={setDraft} onRun={onRun} onSave={vi.fn()}/>;
}

describe("SmartCommandEntry",()=>{
  beforeEach(()=>{vi.restoreAllMocks();workspaceRuntime.cwdBySession.set(session.id,"/目录");});

  it("shows ranked history and snippet suggestions and accepts with the keyboard",async()=>{
    const user=userEvent.setup();render(<Harness/>);const input=screen.getByRole("combobox",{name:"智能命令输入"});
    await user.type(input,"sys");expect(screen.getByRole("option",{name:/systemctl status nginx/})).toBeInTheDocument();
    await user.keyboard("{ArrowDown}{Tab}");expect(input).toHaveValue("systemctl status nginx");
    await user.clear(input);await user.type(input,"journal");expect(screen.getByRole("option",{name:/查看服务日志/})).toBeInTheDocument();
  });

  it("completes a remote path containing Chinese, spaces and quotes safely",async()=>{
    const user=userEvent.setup();vi.spyOn(api,"listFiles").mockResolvedValue([{name:"有 空格'文件",path:"/目录/有 空格'文件",kind:"file",size:1,modifiedAt:null,permissions:"-rw-r--r--",owner:0,group:0}]);
    render(<Harness/>);const input=screen.getByRole("combobox",{name:"智能命令输入"});await user.type(input,"cat /目录/有");
    const option=await screen.findByRole("option",{name:/有 空格'文件/});await user.click(option);
    expect(input).toHaveValue("cat '/目录/有 空格'\\''文件'");
  });

  it("executes the draft when no suggestion is selected",async()=>{
    const run=vi.fn();const user=userEvent.setup();render(<Harness history={[]} onRun={run}/>);const input=screen.getByRole("combobox",{name:"智能命令输入"});
    await user.type(input,"echo ok{Enter}");await waitFor(()=>expect(run).toHaveBeenCalledWith("echo ok"));
  });
});
