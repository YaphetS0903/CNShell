export function parseRemoteMode(value: string): number | null {
  const normalized = value.trim();
  return /^[0-7]{3,4}$/.test(normalized) ? Number.parseInt(normalized, 8) : null;
}
