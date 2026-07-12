import { render,screen,waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach,describe,expect,it,vi } from "vitest";
import { api } from "../../lib/api";
import type { SessionLogStatus,TerminalSession } from "../../types";
import { SessionLogDialog } from "./SessionLogDialog";

const dialog=vi.hoisted(()=>({save:vi.fn()}));
vi.mock("@tauri-apps/plugin-dialog",()=>dialog);
const session:TerminalSession={id:"session-1",connectionId:"connection-1",sessionType:"terminal",title:"生产主机",status:"online",startedAt:"",lastError:null};
const inactive:SessionLogStatus={sessionId:session.id,active:false,path:null,format:null,lineTimestamps:false,startedAt:null,bytesWritten:0,error:null};
const active:SessionLogStatus={sessionId:session.id,active:true,path:"/private/logs/session.log",format:"text",lineTimestamps:true,startedAt:"2026-07-12T10:00:00Z",bytesWritten:2048,error:null};

describe("SessionLogDialog",()=>{
  beforeEach(()=>{vi.restoreAllMocks();dialog.save.mockReset();localStorage.removeItem("cnshell-session-log-preferences");vi.spyOn(api,"sessionLogStatus").mockResolvedValue(inactive);});

  it("warns about secrets and starts with bounded retention settings",async()=>{
    const start=vi.spyOn(api,"startSessionLog").mockResolvedValue({...active,format:"jsonl",lineTimestamps:false});const user=userEvent.setup();render(<SessionLogDialog session={session} onClose={vi.fn()} onError={vi.fn()}/>);
    expect(screen.getByText(/日志可能包含命令、主机信息、令牌或其他秘密/)).toBeInTheDocument();await user.selectOptions(screen.getByRole("combobox"),"jsonl");
    expect(screen.getByRole("checkbox")).toBeDisabled();const numbers=screen.getAllByRole("spinbutton");await user.clear(numbers[0]);await user.type(numbers[0],"45");await user.clear(numbers[1]);await user.type(numbers[1],"256");await user.click(screen.getByRole("button",{name:"开始记录"}));
    await waitFor(()=>expect(start).toHaveBeenCalledWith(session.id,"jsonl",false,45,256*1024*1024));expect(screen.getByText("正在记录")).toBeInTheDocument();
  });

  it("stops and exports an existing log",async()=>{
    vi.spyOn(api,"sessionLogStatus").mockResolvedValue(active);const stop=vi.spyOn(api,"stopSessionLog").mockResolvedValue({...active,active:false});const exportLog=vi.spyOn(api,"exportSessionLog").mockResolvedValue();dialog.save.mockResolvedValue("/Users/test/production.log");const user=userEvent.setup();render(<SessionLogDialog session={session} onClose={vi.fn()} onError={vi.fn()}/>);
    expect(await screen.findByText("正在记录")).toBeInTheDocument();await user.click(screen.getByRole("button",{name:"导出"}));await waitFor(()=>expect(exportLog).toHaveBeenCalledWith(session.id,"/Users/test/production.log"));await user.click(screen.getByRole("button",{name:"停止记录"}));await waitFor(()=>expect(stop).toHaveBeenCalledWith(session.id));
  });
});
