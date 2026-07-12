export type SuggestionKind = "history" | "snippet" | "path";
export type CommandSuggestion = { id:string; kind:SuggestionKind; label:string; detail:string; value:string };

const normalize=(value:string)=>value.toLocaleLowerCase("zh-CN");

export function fuzzyScore(query:string,value:string):number|null {
  const needle=normalize(query.trim());const haystack=normalize(value);
  if(!needle)return 0;
  const exact=haystack.indexOf(needle);if(exact>=0)return exact===0?1000-needle.length:700-exact;
  let cursor=0;let gaps=0;
  for(const character of needle){const found=haystack.indexOf(character,cursor);if(found<0)return null;gaps+=found-cursor;cursor=found+1;}
  return 400-gaps;
}

export function rankCommandSuggestions(query:string,items:CommandSuggestion[],limit=10):CommandSuggestion[] {
  return items.map((item)=>({item,score:Math.max(fuzzyScore(query,item.label)??-1,fuzzyScore(query,item.detail)??-1)})).filter(({score})=>score>=0).sort((a,b)=>b.score-a.score||a.item.label.localeCompare(b.item.label,"zh-CN")).slice(0,limit).map(({item})=>item);
}

export function completionToken(command:string):{start:number;value:string}|null {
  let start=0;let quote:"'"|'"'|null=null;let escaped=false;
  for(let index=0;index<command.length;index+=1){const character=command[index];if(escaped){escaped=false;continue;}if(character==="\\"&&quote!=="'"){escaped=true;continue;}if(character==="'"||character==='"'){if(quote===character)quote=null;else if(!quote)quote=character;continue;}if(/\s/.test(character)&&!quote)start=index+1;}
  return{start,value:command.slice(start)};
}

export function replaceCompletionToken(command:string,replacement:string):string {
  const token=completionToken(command);return token?`${command.slice(0,token.start)}${replacement}`:command;
}

export function templateParameters(template:string):string[] {
  return [...template.matchAll(/\{\{\s*([a-zA-Z][\w.-]{0,63})\s*\}\}/g)].map((match)=>match[1]).filter((value,index,all)=>all.indexOf(value)===index);
}

export function shellQuote(value:string):string {return `'${value.replaceAll("'","'\\''")}'`;}

export function renderCommandTemplate(template:string,parameters:Record<string,string>):string {
  const required=templateParameters(template);for(const name of required)if(!(name in parameters))throw new Error(`缺少参数 ${name}`);
  const rendered=template.replace(/\{\{\s*([a-zA-Z][\w.-]{0,63})\s*\}\}/g,(_,name:string)=>shellQuote(parameters[name]??""));
  if(/\{\{.*\}\}/.test(rendered))throw new Error("模板包含无效参数占位符");
  return rendered;
}

export function isHighRiskCommand(command:string):boolean {
  const normalized=command.trim().toLowerCase();
  return /(^|[;&|]\s*)(rm\s+-[^\n]*(r[^\n]*f|f[^\n]*r)[^\n]*\s+\/?($|\s)|mkfs(\.|\s)|shutdown\s|reboot($|\s)|poweroff($|\s)|dd\s+[^\n]*of=\/dev\/|chmod\s+-r\s+777\s+\/)/.test(normalized);
}

export function pathLookup(command:string,cwd:string):{parent:string;prefix:string}|null {
  const token=completionToken(command)?.value??"";const unquoted=decodeShellToken(token);if(!unquoted||(!unquoted.includes("/")&&!unquoted.startsWith(".")))return null;
  const absolute=unquoted.startsWith("/")?unquoted:`${cwd.replace(/\/$/,"")}/${unquoted}`;
  const slash=absolute.lastIndexOf("/");return{parent:absolute.slice(0,slash)||"/",prefix:absolute.slice(slash+1)};
}

function decodeShellToken(value:string):string {
  let result="";let quote:"'"|'"'|null=null;let escaped=false;
  for(const character of value){if(escaped){result+=character;escaped=false;continue;}if(character==="\\"&&quote!=="'"){escaped=true;continue;}if(character==="'"||character==='"'){if(quote===character)quote=null;else if(!quote)quote=character;else result+=character;continue;}result+=character;}
  if(escaped)result+="\\";return result;
}
