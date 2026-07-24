export interface NativeDropPosition {
  x: number;
  y: number;
}

type DropZone = {
  getBoundingClientRect: () => Pick<DOMRect, "left" | "top" | "right" | "bottom">;
};

/**
 * Tauri reports native file-drop coordinates in physical pixels on Windows,
 * while macOS WebKit can report the WebView's point coordinates.  Accept both
 * representations so the hit test remains correct on Retina and scaled
 * Windows displays.
 */
export function nativeDropIsInsideElement(
  position: NativeDropPosition,
  element: DropZone,
  devicePixelRatio = window.devicePixelRatio,
): boolean {
  const rect = element.getBoundingClientRect();
  const scale = Number.isFinite(devicePixelRatio) && devicePixelRatio > 0 ? devicePixelRatio : 1;
  const candidates = [
    position,
    { x: position.x / scale, y: position.y / scale },
  ];

  return candidates.some(({ x, y }) =>
    Number.isFinite(x) &&
    Number.isFinite(y) &&
    x >= rect.left &&
    x <= rect.right &&
    y >= rect.top &&
    y <= rect.bottom,
  );
}
