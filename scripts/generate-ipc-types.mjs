import { readFile, writeFile } from "node:fs/promises";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const sourcePath = resolve(root, "src-tauri/src/models.rs");
const outputPath = resolve(root, "src/generated/ipc.ts");
const source = await readFile(sourcePath, "utf8");
const normalizeLineEndings = (value) => value.replace(/\r\n/g, "\n");

const camelCase = (value) => value.replace(/_([a-z])/g, (_, letter) => letter.toUpperCase());
const splitGeneric = (value) => {
  let depth = 0;
  for (let index = 0; index < value.length; index += 1) {
    if (value[index] === "<" || value[index] === "[") depth += 1;
    if (value[index] === ">" || value[index] === "]") depth -= 1;
    if (value[index] === "," && depth === 0) return [value.slice(0, index), value.slice(index + 1)];
  }
  return [value];
};
const typeScriptType = (rustType) => {
  const type = rustType.trim();
  if (type === "String" || type === "str") return "string";
  if (type === "serde_json::Value") return "unknown";
  if (type === "bool") return "boolean";
  if (/^(u|i)(8|16|32|64|128|size)$/.test(type) || /^f(32|64)$/.test(type)) return "number";
  if (type.startsWith("Option<") && type.endsWith(">")) return `${typeScriptType(type.slice(7, -1))} | null`;
  if (type.startsWith("Vec<") && type.endsWith(">")) return `${typeScriptType(type.slice(4, -1))}[]`;
  if (type.startsWith("std::collections::BTreeMap<") && type.endsWith(">")) {
    const [key, value] = splitGeneric(type.slice(27, -1));
    return `Record<${typeScriptType(key)}, ${typeScriptType(value)}>`;
  }
  const array = type.match(/^\[([^;]+);\s*(\d+)\]$/);
  if (array) return `[${Array.from({ length: Number(array[2]) }, () => typeScriptType(array[1])).join(", ")}]`;
  return type.replace(/^.*::/, "");
};

const structs = [];
const structPattern = /((?:#\[[^\n]+\]\s*)*)pub struct (\w+)\s*\{/g;
for (let match = structPattern.exec(source); match; match = structPattern.exec(source)) {
  let depth = 1;
  let cursor = structPattern.lastIndex;
  while (cursor < source.length && depth > 0) {
    if (source[cursor] === "{") depth += 1;
    if (source[cursor] === "}") depth -= 1;
    cursor += 1;
  }
  if (depth !== 0) throw new Error(`Unclosed Rust struct ${match[2]}`);
  const attributes = match[1];
  const body = source.slice(structPattern.lastIndex, cursor - 1);
  const serializes = attributes.includes("Serialize");
  const deserializes = attributes.includes("Deserialize");
  if (!serializes && !deserializes) continue;
  const fields = [];
  const fieldPattern = /((?:\s*#\[[^\n]+\]\s*)*)\s*pub (\w+):\s*([^\n]+),/g;
  for (let field = fieldPattern.exec(body); field; field = fieldPattern.exec(body)) {
    const fieldAttributes = field[1];
    const rustName = field[2];
    const rustType = field[3].trim();
    const rename = fieldAttributes.match(/serde\(rename\s*=\s*"([^"]+)"\)/)?.[1];
    const optional = rustType.startsWith("Option<") && deserializes && !serializes;
    fields.push({
      name: rename ?? camelCase(rustName),
      optional: optional || fieldAttributes.includes("skip_serializing_if"),
      type: typeScriptType(rustType),
    });
  }
  structs.push({ name: match[2], fields });
  structPattern.lastIndex = cursor;
}

const output = `// Generated from src-tauri/src/models.rs by scripts/generate-ipc-types.mjs.\n// Do not edit directly; run npm run generate:ipc.\n\n${structs.map(({ name, fields }) => `export interface ${name} {\n${fields.map((field) => `  ${field.name}${field.optional ? "?" : ""}: ${field.type};`).join("\n")}\n}`).join("\n\n")}\n`;

if (process.argv.includes("--check")) {
  const current = await readFile(outputPath, "utf8").catch(() => "");
  if (normalizeLineEndings(current) !== output) {
    console.error("IPC types are stale. Run: npm run generate:ipc");
    process.exitCode = 1;
  }
} else {
  await writeFile(outputPath, output);
  console.log(`Generated ${structs.length} IPC types in ${outputPath.slice(root.length + 1)}`);
}
