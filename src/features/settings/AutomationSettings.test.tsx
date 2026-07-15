import { render,screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach,describe,expect,it,vi } from "vitest";
import { api } from "../../lib/api";
import type { ConnectionProfile } from "../../types";
import { AutomationSettings } from "./AutomationSettings";

const connection:ConnectionProfile={id:"server",folderId:null,protocol:"ssh",name:"服务器",host:"example",port:22,username:"root",authType:"sshAgent",privateKeyPath:null, certificatePath: null,hostKeyPolicy:"strict",note:"",tags:[],encoding:"UTF-8",startupCommand:null,proxyId:null,environment:{},hasCredential:false,createdAt:"",updatedAt:"",lastConnectedAt:null};
describe("AutomationSettings",()=>{beforeEach(()=>{vi.spyOn(api,"validateAutomation").mockImplementation(async(plan)=>plan);vi.spyOn(api,"startAutomation").mockResolvedValue({id:"task",kind:"automation",status:"queued",result:null,error:null,createdAt:"now"});vi.spyOn(window,"confirm").mockReturnValue(true);});it("previews the final workflow before starting",async()=>{const user=userEvent.setup();render(<AutomationSettings connections={[connection]} onError={()=>undefined}/>);await user.type(screen.getByRole("textbox",{name:"计划名称"}),"检查");await user.selectOptions(screen.getByRole("combobox",{name:"目标连接"}),"server");await user.type(screen.getByRole("textbox",{name:"命令"}),"uname -a");expect(screen.getByLabelText("自动化预览")).toHaveTextContent("执行 uname -a");await user.click(screen.getByRole("button",{name:"预览并运行"}));expect(window.confirm).toHaveBeenCalledWith(expect.stringContaining("uname -a"));expect(api.startAutomation).toHaveBeenCalled();});});
