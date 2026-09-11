export function nextConnectionCopyName(
  sourceName: string,
  existingNames: Iterable<string>,
) {
  const base = sourceName.replace(/ 副本(?: \d+)?$/u, "");
  const names = new Set(existingNames);
  const first = `${base} 副本`;
  if (!names.has(first)) return first;
  let index = 2;
  while (names.has(`${base} 副本 ${index}`)) index += 1;
  return `${base} 副本 ${index}`;
}
