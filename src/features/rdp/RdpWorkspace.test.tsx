import { render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { RdpWorkspace } from "./RdpWorkspace";

describe("RdpWorkspace", () => {
  it("exposes managed helper status and close action", () => {
    render(<RdpWorkspace session={{id:"rdp",connectionId:"connection",sessionType:"rdp",title:"Windows",status:"online",startedAt:"now",lastError:null}} onReconnect={vi.fn()} onClose={vi.fn()}/>);
    expect(screen.getByRole("region", { name: "RDP 会话 Windows" })).toHaveTextContent("FreeRDP");
    expect(screen.getByRole("button", { name: /关闭远程桌面/ })).toBeInTheDocument();
  });
});
