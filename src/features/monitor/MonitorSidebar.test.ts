import { describe, expect, it } from "vitest";
import { appendMonitorSample } from "../../lib/runtime-metrics";

describe("monitor history",()=>{
  it("keeps five minutes at the configured interval",()=>{let history:number[]=[];for(let index=0;index<200;index++)history=appendMonitorSample(history,index,2_000);expect(history).toHaveLength(150);expect(history[0]).toBe(50);});
});
