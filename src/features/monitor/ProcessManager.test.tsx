import { render,screen,waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach,describe,expect,it,vi } from "vitest";
import { api } from "../../lib/api";
import type { ProcessInfo } from "../../types";
import { ProcessManager } from "./ProcessManager";

const process:ProcessInfo={pid:123,startedAt:"Sun Jul 12 10:00:00 2026",user:"root",cpuPercent:25,memoryPercent:10,command:"python3 /opt/service.py --token redacted"};
describe("ProcessManager",()=>{beforeEach(()=>{vi.restoreAllMocks();vi.spyOn(window,"confirm").mockReturnValue(true);vi.spyOn(api,"signalProcess").mockResolvedValue(undefined);});it("filters by full command and sends a signal with immutable identity",async()=>{const refresh=vi.fn().mockResolvedValue(undefined);const user=userEvent.setup();render(<ProcessManager sessionId="session-1" processes={[process,{...process,pid:456,command:"nginx"}]} onRefresh={refresh} onError={vi.fn()}/>);await user.type(screen.getByRole("textbox",{name:"搜索进程"}),"service.py");expect(screen.getByText("python3 /opt/service.py --token redacted")).toBeInTheDocument();expect(screen.queryByText("nginx")).not.toBeInTheDocument();await user.click(screen.getByRole("button",{name:/TERM/}));await waitFor(()=>expect(api.signalProcess).toHaveBeenCalledWith("session-1",process,"TERM"));expect(refresh).toHaveBeenCalled();});});
