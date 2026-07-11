import { describe, expect, it } from "vitest";
import { virtualWindow } from "../../lib/runtime-metrics";

describe("remote file virtualization",()=>{
  it("renders only a bounded window in a 100,000 item directory",()=>{const range=virtualWindow(100_000,26*50_000,520);expect(range.start).toBe(49_990);expect(range.end-range.start).toBeLessThanOrEqual(40);expect(range.top+range.bottom+(range.end-range.start)*26).toBe(100_000*26);});
});
