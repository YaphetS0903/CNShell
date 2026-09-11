import { describe, expect, it } from "vitest";
import type { TransferTask } from "../../types";
import { updateTransferMetric } from "../../lib/runtime-metrics";
import { friendlyTransferError } from "../../lib/transfer-errors";

const task=(bytes:number):TransferTask=>({id:"transfer",sessionId:"session",direction:"download",source:"/remote",destination:"/local",totalBytes:1_000,status:"running",transferredBytes:bytes,conflictPolicy:"overwrite",error:null,createdAt:"now"});

describe("transfer metrics",()=>{
  it("calculates speed and remaining time from progress events",()=>{const first=updateTransferMetric(undefined,task(100),1_000);const second=updateTransferMetric(first,task(300),2_000);expect(second.speed).toBe(200);expect(second.etaSeconds).toBe(3.5);});
  it("turns backend transfer errors into actionable messages",()=>{expect(friendlyTransferError("SFTP(3): Permission denied")).toContain("目标目录不可写");expect(friendlyTransferError("session 0d9a was not found")).toContain("重新连接");expect(friendlyTransferError("unexpected backend failure")).toContain("技术详情");});
});
