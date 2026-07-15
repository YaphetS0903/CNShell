export const workspaceRuntime={
  cwdBySession:new Map<string,string>(),
  terminalLayout:null as import("../features/terminal/terminal-layout").TerminalLayout|null,
  bottomOpen:true,
  bottomHeight:260,
  triggerEventsBySession:new Map<string,import("../features/terminal/terminal-triggers").TriggerEvent[]>(),
  pasteHistory:[] as string[],
  terminalSearchBySession:new Map<string,(query:string)=>import("../features/terminal/terminal-safety").GlobalSearchMatch[]>(),
  terminalTimestampsBySession:new Map<string,import("../features/terminal/terminal-safety").TerminalLineTimestamp[]>(),
  terminalActivity:new Set<string>(),
  terminalSelectionBySession:new Map<string,string>(),
  remoteFileBrowserBySession:new Map<string,{path:string;expandedPaths:string[]}>()
};

export function parseOsc7Cwd(value:string):string|null{
  try{const url=new URL(value);if(url.protocol!=="file:")return null;const path=decodeURIComponent(url.pathname);return path.startsWith("/")&&path.length<=16*1024&&!path.includes("\0")?path:null;}catch{return null;}
}

export function shellQuote(value:string){return `'${value.replaceAll("'","'\\''")}'`;}
