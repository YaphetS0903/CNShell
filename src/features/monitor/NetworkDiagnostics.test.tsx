import { render,screen,waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach,describe,expect,it,vi } from "vitest";
import { api } from "../../lib/api";
import type { BackgroundTask } from "../../types";
import { NetworkDiagnostics } from "./NetworkDiagnostics";

const task:BackgroundTask={id:"task-1",kind:"networkDiagnostic",status:"completed",result:{kind:"ping",target:"example.com",output:"rtt min/avg/max/mdev = 10.0/20.0/30.0/1.0 ms",durationMs:50},error:null,createdAt:""};
describe("NetworkDiagnostics",()=>{beforeEach(()=>{vi.restoreAllMocks();vi.spyOn(api,"networkSockets").mockResolvedValue({items:[{protocol:"tcp",state:"LISTEN",localAddress:"0.0.0.0:22",peerAddress:"0.0.0.0:*",process:"sshd"}],warning:null});vi.spyOn(api,"startNetworkDiagnostic").mockResolvedValue(task);vi.spyOn(api,"onBackgroundTask").mockResolvedValue(()=>undefined);vi.spyOn(api,"getTask").mockResolvedValue(task);});it("shows sockets and runs a cancellable ping task",async()=>{const user=userEvent.setup();render(<NetworkDiagnostics sessionId="session-1" onError={vi.fn()}/>);expect(await screen.findByText("0.0.0.0:22")).toBeInTheDocument();await user.type(screen.getByRole("textbox",{name:"网络诊断目标"}),"example.com");await user.click(screen.getByRole("button",{name:"Ping"}));await waitFor(()=>expect(api.startNetworkDiagnostic).toHaveBeenCalledWith("session-1","ping","example.com"));expect(await screen.findByText(/Ping · example.com/)).toBeInTheDocument();expect(screen.getByText(/20.0\/30.0/)).toBeInTheDocument();});});
