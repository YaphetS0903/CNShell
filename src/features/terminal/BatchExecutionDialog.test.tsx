import { render,screen,waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach,describe,expect,it,vi } from "vitest";
import { api } from "../../lib/api";
import { useAppStore } from "../../store/app-store";
import type { BatchExecution,ConnectionProfile,TerminalSession } from "../../types";
import { BatchExecutionDialog } from "./BatchExecutionDialog";

const connection=(index:number):ConnectionProfile=>({id:`connection-${index}`,folderId:null,protocol:"ssh",name:`主机 ${index}`,host:`10.0.0.${index}`,port:22,username:"root",authType:"sshAgent",privateKeyPath:null,hostKeyPolicy:"strict",note:"",tags:[],encoding:"UTF-8",startupCommand:null,proxyId:null,environment:{},hasCredential:false,createdAt:"",updatedAt:"",lastConnectedAt:null});
const execution:BatchExecution={id:"batch-1",command:"uname -a",status:"failed",createdAt:"",targets:[{connectionId:"connection-1",name:"主机 1",status:"completed",stdout:"Linux",stderr:"",exitCode:0,durationMs:12,error:null},{connectionId:"connection-2",name:"主机 2",status:"failed",stdout:"",stderr:"denied",exitCode:1,durationMs:18,error:null}]};

describe("BatchExecutionDialog",()=>{
  beforeEach(()=>{vi.restoreAllMocks();useAppStore.setState({sessions:[]});vi.spyOn(api,"onBatchExecution").mockResolvedValue(()=>undefined);vi.spyOn(api,"startBatch").mockResolvedValue(execution);vi.spyOn(api,"terminalInput").mockResolvedValue();});

  it("previews 20 targets without collapsing failures into other results",async()=>{
    const user=userEvent.setup();const items=Array.from({length:20},(_,index)=>connection(index+1));render(<BatchExecutionDialog connections={items} connect={vi.fn()} onClose={vi.fn()} onError={vi.fn()}/>);await user.click(screen.getByText("选择全部"));await user.type(screen.getByPlaceholderText("输入要在所有目标执行的命令"),"uname -a");await user.click(screen.getByRole("button",{name:"预览执行"}));expect(screen.getByText(/以下 20 台主机执行/)).toBeInTheDocument();await user.click(screen.getByRole("button",{name:"确认执行"}));await waitFor(()=>expect(api.startBatch).toHaveBeenCalledWith(items.map((item)=>item.id),"uname -a",4));expect(await screen.findByText("1/2 成功")).toBeInTheDocument();await user.click(screen.getByRole("button",{name:/主机 2/}));expect(screen.getByText("denied")).toBeInTheDocument();
  });

  it("retries only failed targets",async()=>{const user=userEvent.setup();render(<BatchExecutionDialog connections={[connection(1),connection(2)]} connect={vi.fn()} onClose={vi.fn()} onError={vi.fn()}/>);await user.click(screen.getByText("选择全部"));await user.type(screen.getByPlaceholderText("输入要在所有目标执行的命令"),"uname -a");await user.click(screen.getByRole("button",{name:"预览执行"}));await user.click(screen.getByRole("button",{name:"确认执行"}));await screen.findByText("1/2 成功");vi.mocked(api.startBatch).mockClear();await user.click(screen.getByRole("button",{name:"仅重试失败项"}));await waitFor(()=>expect(api.startBatch).toHaveBeenCalledWith(["connection-2"],"uname -a",4));});

  it("opens selected sessions and sends synchronized input",async()=>{const user=userEvent.setup();const items=[connection(1),connection(2)];const connect=vi.fn(async(profile:ConnectionProfile)=>{const session:TerminalSession={id:`session-${profile.id}`,connectionId:profile.id,sessionType:"terminal",title:profile.name,status:"online",startedAt:"",lastError:null};useAppStore.getState().addSession(session);});render(<BatchExecutionDialog connections={items} connect={connect} onClose={vi.fn()} onError={vi.fn()}/>);await user.click(screen.getByText("选择全部"));await user.click(screen.getByRole("tab",{name:"同步输入"}));await user.click(screen.getByRole("button",{name:/建立 2 个同步会话/}));expect(await screen.findByText("2 台主机已就绪")).toBeInTheDocument();await user.type(screen.getByRole("textbox",{name:"同步命令"}),"uptime");await user.click(screen.getByRole("button",{name:"发送"}));await waitFor(()=>expect(api.terminalInput).toHaveBeenCalledTimes(2));expect(api.terminalInput).toHaveBeenCalledWith("session-connection-1","uptime\r");});
});
