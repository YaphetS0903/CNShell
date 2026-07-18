export function localPathName(path: string): string {
  const trimmed = path.replace(/[\\/]+$/, "");
  return trimmed.split(/[\\/]/).at(-1) ?? trimmed;
}

export function joinLocalPath(
  directory: string,
  name: string,
  operatingSystem: string,
): string {
  const separator = operatingSystem === "windows" ? "\\" : "/";
  const parent = directory.replace(/[\\/]+$/, "");
  return `${parent}${separator}${name}`;
}
