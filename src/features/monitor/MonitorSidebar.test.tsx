import { afterEach, describe, expect, it, vi } from "vitest";
import { appendMonitorHistory, appendMonitorSample, shouldReportMonitorPollError } from "../../lib/runtime-metrics";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { DiskItem } from "./DiskItem";
import { useAppStore } from "../../store/app-store";
import { MonitorSidebar } from "./MonitorSidebar";

vi.mock("./MonitorHistoryChart",()=>({MonitorHistoryChart:()=>null}));

describe("monitor history",()=>{
  afterEach(()=>useAppStore.setState({sessions:[],activeSessionId:null,monitor:null}));
  it("keeps five minutes at the configured interval",()=>{let history:number[]=[];for(let index=0;index<200;index++)history=appendMonitorSample(history,index,2_000);expect(history).toHaveLength(150);expect(history[0]).toBe(50);});
  it("keeps aligned CPU, network and latency samples",()=>{let history:ReturnType<typeof appendMonitorHistory>=[];for(let index=0;index<200;index++)history=appendMonitorHistory(history,{timestamp:index,cpu:index,received:index*2,sent:index*3,latency:index},2_000);expect(history).toHaveLength(150);expect(history[0]).toMatchObject({cpu:50,received:100,sent:150,latency:50});});
  it("shows total, used and available disk space",()=>{render(<DiskItem disk={{filesystem:"/dev/disk",mountPoint:"/",totalBytes:100*1024**3,usedBytes:82*1024**3,availableBytes:18*1024**3,usedPercent:82}}/>);expect(screen.getByText("已用 82.0 GB / 100.0 GB · 剩余 18.0 GB")).toBeVisible();expect(screen.getByLabelText(/总量 100.0 GB/)).toBeVisible();});
  it("renders every disk in a keyboard-scrollable monitor region",()=>{const disks=Array.from({length:8},(_,index)=>({filesystem:`disk-${index}`,mountPoint:`/mount-${index}`,totalBytes:100,usedBytes:50,availableBytes:50,usedPercent:50}));useAppStore.setState({sessions:[{id:"session",connectionId:"connection",sessionType:"terminal",title:"server",status:"online",startedAt:"",lastError:null}],activeSessionId:"session",monitor:{sessionId:"session",timestamp:0,hostname:"server",ip:"127.0.0.1",uptimeSeconds:0,load:[0,0,0],cpuPercent:0,memoryUsedBytes:0,memoryTotalBytes:0,swapUsedBytes:0,swapTotalBytes:0,latencyMs:null,processes:[],disks,networks:[],warnings:[]}});render(<MonitorSidebar/>);expect(screen.getByLabelText("服务器监控详情")).toHaveAttribute("tabindex","0");expect(screen.getByText("/mount-7")).toBeVisible();});
  it("lets users enlarge the monitor text and remembers the choice",async()=>{localStorage.clear();const user=userEvent.setup();useAppStore.setState({sessions:[{id:"session",connectionId:"connection",sessionType:"terminal",title:"server",status:"online",startedAt:"",lastError:null}],activeSessionId:"session",monitor:{sessionId:"session",timestamp:0,hostname:"server",ip:"127.0.0.1",uptimeSeconds:0,load:[0,0,0],cpuPercent:0,memoryUsedBytes:0,memoryTotalBytes:0,swapUsedBytes:0,swapTotalBytes:0,latencyMs:null,processes:[],disks:[],networks:[],warnings:[]}});const first=render(<MonitorSidebar/>);expect(screen.getByText("11px")).toBeVisible();await user.click(screen.getByRole("button",{name:"增大监控字号"}));expect(screen.getByText("12px")).toBeVisible();expect(localStorage.getItem("cnshell-monitor-font-size")).toBe("12");first.unmount();render(<MonitorSidebar/>);expect(screen.getByText("12px")).toBeVisible();});
  it("suppresses transient polling errors while Mosh reconnects",()=>{expect(shouldReportMonitorPollError({sessionType:"mosh",status:"reconnecting"},10)).toBe(false);expect(shouldReportMonitorPollError({sessionType:"mosh",status:"online"},2)).toBe(false);expect(shouldReportMonitorPollError({sessionType:"mosh",status:"online"},3)).toBe(true);});
});
