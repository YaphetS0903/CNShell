import { describe, expect, it } from "vitest";
import { appendMonitorHistory, appendMonitorSample } from "../../lib/runtime-metrics";
import { render, screen } from "@testing-library/react";
import { DiskItem } from "./DiskItem";

describe("monitor history",()=>{
  it("keeps five minutes at the configured interval",()=>{let history:number[]=[];for(let index=0;index<200;index++)history=appendMonitorSample(history,index,2_000);expect(history).toHaveLength(150);expect(history[0]).toBe(50);});
  it("keeps aligned CPU, network and latency samples",()=>{let history:ReturnType<typeof appendMonitorHistory>=[];for(let index=0;index<200;index++)history=appendMonitorHistory(history,{timestamp:index,cpu:index,received:index*2,sent:index*3,latency:index},2_000);expect(history).toHaveLength(150);expect(history[0]).toMatchObject({cpu:50,received:100,sent:150,latency:50});});
  it("shows total, used and available disk space",()=>{render(<DiskItem disk={{filesystem:"/dev/disk",mountPoint:"/",totalBytes:100*1024**3,usedBytes:82*1024**3,availableBytes:18*1024**3,usedPercent:82}}/>);expect(screen.getByText("已用 82.0 GB / 100.0 GB · 剩余 18.0 GB")).toBeVisible();expect(screen.getByLabelText(/总量 100.0 GB/)).toBeVisible();});
});
