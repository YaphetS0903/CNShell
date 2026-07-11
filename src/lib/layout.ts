export const clampPanelSize = (value: number, minimum: number, maximum: number) => Math.min(maximum, Math.max(minimum, Math.round(value)));

export const resizeFromKeyboard = (current: number, key: string, orientation: "horizontal" | "vertical", step = 16) => {
  if (orientation === "vertical" && key === "ArrowLeft" || orientation === "horizontal" && key === "ArrowUp") return current - step;
  if (orientation === "vertical" && key === "ArrowRight" || orientation === "horizontal" && key === "ArrowDown") return current + step;
  return current;
};
