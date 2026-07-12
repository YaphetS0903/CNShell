import { AlertTriangle, Play } from "lucide-react";
import { useMemo, useState } from "react";
import { Modal } from "../../components/Modal";
import { renderCommandTemplate, templateParameters } from "./smart-command";

export function CommandTemplateDialog({template,onClose,onRun}:{template:string;onClose:()=>void;onRun:(command:string)=>void}){
  const names=useMemo(()=>templateParameters(template),[template]);
  const[values,setValues]=useState<Record<string,string>>(()=>Object.fromEntries(names.map((name)=>[name,""])));
  const preview=renderCommandTemplate(template,values);
  return <Modal title="填写命令参数" onClose={onClose}>
    <form className="command-template-form" onSubmit={(event)=>{event.preventDefault();onRun(preview);}}>
      <p className="form-hint"><AlertTriangle size={15}/>参数会自动进行 Shell 转义，请在执行前核对最终命令。</p>
      {names.map((name)=><label key={name}><span>{name}</span><input autoFocus={name===names[0]} value={values[name]??""} onChange={(event)=>setValues((current)=>({...current,[name]:event.target.value}))} required aria-label={`命令参数 ${name}`}/></label>)}
      <div className="command-preview"><span>最终命令</span><code>{preview}</code></div>
      <footer className="form-actions"><button type="button" className="button secondary" onClick={onClose}>取消</button><button className="button primary"><Play size={14}/>执行</button></footer>
    </form>
  </Modal>;
}
