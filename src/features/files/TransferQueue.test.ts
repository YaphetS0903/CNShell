import { describe, expect, it } from "vitest";
import type { TransferTask } from "../../types";
import { updateTransferMetric } from "../../lib/runtime-metrics";

const task=(bytes:number):TransferTask=>({id:"transfer",sessionId:"session",direction:"download",source:"/remote",destination:"/local",totalBytes:1_000,status:"running",transferredBytes:bytes,conflictPolicy:"overwrite",error:null,createdAt:"now"});

describe("transfer metrics",()=>{
  it("calculates speed and remaining time from progress events",()=>{const first=updateTransferMetric(undefined,task(100),1_000);const second=updateTransferMetric(first,task(300),2_000);expect(second.speed).toBe(200);expect(second.etaSeconds).toBe(3.5);});
});
