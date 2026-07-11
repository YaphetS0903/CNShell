import { describe, expect, it } from "vitest";
import { appendMonitorHistory, appendMonitorSample } from "../../lib/runtime-metrics";

describe("monitor history",()=>{
  it("keeps five minutes at the configured interval",()=>{let history:number[]=[];for(let index=0;index<200;index++)history=appendMonitorSample(history,index,2_000);expect(history).toHaveLength(150);expect(history[0]).toBe(50);});
  it("keeps aligned CPU, network and latency samples",()=>{let history:ReturnType<typeof appendMonitorHistory>=[];for(let index=0;index<200;index++)history=appendMonitorHistory(history,{timestamp:index,cpu:index,received:index*2,sent:index*3,latency:index},2_000);expect(history).toHaveLength(150);expect(history[0]).toMatchObject({cpu:50,received:100,sent:150,latency:50});});
});
