export const workspaceRuntime={
  cwdBySession:new Map<string,string>(),
  splitSessionId:null as string|null,
  bottomOpen:true,
  bottomHeight:260
};

export function parseOsc7Cwd(value:string):string|null{
  try{const url=new URL(value);if(url.protocol!=="file:")return null;const path=decodeURIComponent(url.pathname);return path.startsWith("/")&&path.length<=16*1024&&!path.includes("\0")?path:null;}catch{return null;}
}

export function shellQuote(value:string){return `'${value.replaceAll("'","'\\''")}'`;}
