import { beforeEach, describe, expect, it } from "vitest";
import {
  accessibleRuleColors,
  defaultTriggerConfig,
  findTriggerMatches,
  loadTriggerConfig,
  saveTriggerConfig,
  terminalCellWidth,
  validateTriggerPattern,
} from "./terminal-triggers";

describe("terminal triggers", () => {
  beforeEach(() => localStorage.clear());

  it("accepts built-ins and rejects unsafe or oversized regex", () => {
    for (const rule of defaultTriggerConfig.rules)
      expect(validateTriggerPattern(rule.pattern)).toBeNull();
    expect(validateTriggerPattern("(a+)+$")).toMatch(/性能问题/);
    expect(validateTriggerPattern("(a|aa)+$")).toMatch(/性能问题/);
    expect(validateTriggerPattern("(?=secret)")).toMatch(/不支持/);
    expect(validateTriggerPattern("x".repeat(257))).toMatch(/256/);
  });

  it("matches enabled rules with case handling", () => {
    const rules = [{ ...defaultTriggerConfig.rules[0], enabled: true }];
    const matches = findTriggerMatches("FATAL: request failed", rules);
    expect(matches.map((match) => match.text)).toEqual(["FATAL", "failed"]);
    expect(
      findTriggerMatches("warning", [{ ...rules[0], enabled: false }]),
    ).toEqual([]);
  });

  it("maps Chinese and emoji text to terminal cell widths", () => {
    expect(terminalCellWidth("ab中文")).toBe(6);
    expect(terminalCellWidth("x😀")).toBe(3);
  });

  it("enforces readable highlight contrast when enabled", () => {
    expect(accessibleRuleColors("#777777", "#777777", true).foreground).toBe(
      "#000000",
    );
    expect(accessibleRuleColors("#777777", "#777777", false).foreground).toBe(
      "#777777",
    );
  });

  it("scans one megabyte of bounded terminal lines without blocking", () => {
    const lines = Array.from(
      { length: 1024 },
      (_, index) => `service-${index} ok ${"x".repeat(990)} ERROR`,
    );
    const started = performance.now();
    let count = 0;
    for (const line of lines)
      count += findTriggerMatches(line, defaultTriggerConfig.rules, 5).length;
    expect(count).toBe(1024);
    expect(performance.now() - started).toBeLessThan(250);
  });

  it("persists validated local configuration", () => {
    const config = { ...defaultTriggerConfig, backgroundNotifications: true };
    saveTriggerConfig(config);
    expect(loadTriggerConfig().backgroundNotifications).toBe(true);
    localStorage.setItem(
      "cnshell-terminal-triggers-v1",
      JSON.stringify({
        ...config,
        rules: [{ ...config.rules[0], pattern: "(a+)+" }],
      }),
    );
    expect(loadTriggerConfig().rules).toEqual([]);
  });
});
