import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { api } from "../../lib/api";
import { FeedbackSettings } from "./FeedbackSettings";
import { bugReportUrl } from "./feedback-links";

const dialog=vi.hoisted(()=>({save:vi.fn()}));
vi.mock("@tauri-apps/plugin-dialog",()=>dialog);

const environment={appVersion:"0.1.1",operatingSystem:"macos",osVersion:"15.5",architecture:"aarch64"};
const platform={operatingSystem:"macos",architecture:"aarch64",displayName:"macOS",shortcutModifier:"⌘",credentialStoreName:"macOS Keychain",fileManagerName:"Finder",biometricName:"Touch ID",rdp:{available:true,message:"ok"},mosh:{available:true,message:"ok"},kermit:{available:true,message:"ok"},x11:{available:false,message:"missing"},sshAgent:{available:true,message:"ok"},biometric:{available:true,message:"ok"},serial:{available:true,message:"ok"}};

describe("FeedbackSettings",()=>{
  beforeEach(()=>{
    vi.restoreAllMocks();
    vi.spyOn(api,"feedbackEnvironment").mockResolvedValue(environment);
    vi.spyOn(api,"platformCapabilities").mockResolvedValue(platform);
    vi.spyOn(api,"openExternal").mockResolvedValue();
  });
  it("prefills only non-sensitive runtime metadata in a bug report",async()=>{
    render(<FeedbackSettings onError={vi.fn()}/>);
    expect(await screen.findByText(/CNshell 0.1.1 · macOS 15.5 · aarch64/)).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button",{name:"报告问题"}));
    await waitFor(()=>expect(api.openExternal).toHaveBeenCalled());
    const url=String(vi.mocked(api.openExternal).mock.calls[0][0]);
    expect(url).toContain("github.com/YaphetS0903/CNShell/issues/new");
    expect(decodeURIComponent(url)).toContain("版本：0.1.1");
    expect(decodeURIComponent(url)).not.toMatch(/host|username|password|token/i);
  });
  it("builds a deterministic environment-free fallback URL",()=>{
    expect(bugReportUrl(null)).toBe("https://github.com/YaphetS0903/CNShell/issues/new?title=%5BBug%5D+&body=");
  });
  it("reveals a diagnostic only after a successful export",async()=>{
    vi.spyOn(api,"isDesktop").mockReturnValue(true);
    vi.spyOn(api,"exportDiagnostics").mockResolvedValue();
    vi.spyOn(api,"revealDiagnostics").mockResolvedValue();
    dialog.save.mockResolvedValue("/tmp/CNshell-diagnostics.json");
    const user=userEvent.setup();
    render(<FeedbackSettings onError={vi.fn()}/>);
    const reveal=screen.getByRole("button",{name:"在 Finder 中显示"});
    expect(reveal).toBeDisabled();
    await user.click(screen.getByRole("button",{name:"导出脱敏诊断"}));
    await waitFor(()=>expect(api.exportDiagnostics).toHaveBeenCalledWith("/tmp/CNshell-diagnostics.json"));
    expect(reveal).toBeEnabled();
    await user.click(reveal);
    expect(api.revealDiagnostics).toHaveBeenCalledWith("/tmp/CNshell-diagnostics.json");
  });
});
