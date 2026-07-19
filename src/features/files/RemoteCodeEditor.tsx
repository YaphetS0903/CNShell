import { basicSetup } from "codemirror";
import { EditorState, type Extension } from "@codemirror/state";
import { EditorView } from "@codemirror/view";
import { json } from "@codemirror/lang-json";
import { yaml } from "@codemirror/lang-yaml";
import { javascript } from "@codemirror/lang-javascript";
import { python } from "@codemirror/lang-python";
import { markdown } from "@codemirror/lang-markdown";
import { html } from "@codemirror/lang-html";
import { css } from "@codemirror/lang-css";
import { sql } from "@codemirror/lang-sql";
import { StreamLanguage } from "@codemirror/language";
import { shell } from "@codemirror/legacy-modes/mode/shell";
import { forwardRef,useEffect,useImperativeHandle,useRef } from "react";
import { foldAll,unfoldAll } from "@codemirror/language";
import { openSearchPanel } from "@codemirror/search";

export type CodeEditorActions={search:()=>void;fold:()=>void;unfold:()=>void;focus:()=>void};

export const RemoteCodeEditor=forwardRef<CodeEditorActions,{path:string;value:string;onChange:(value:string)=>void}>(({path,value,onChange},ref)=>{
  const hostRef=useRef<HTMLDivElement>(null);const viewRef=useRef<EditorView|null>(null);const initialValueRef=useRef(value);const onChangeRef=useRef(onChange);onChangeRef.current=onChange;
  useImperativeHandle(ref,()=>({search:()=>{const view=viewRef.current;if(view)openSearchPanel(view);},fold:()=>{const view=viewRef.current;if(view)foldAll(view);},unfold:()=>{const view=viewRef.current;if(view)unfoldAll(view);},focus:()=>viewRef.current?.focus()}),[]);
  useEffect(()=>{const host=hostRef.current;if(!host)return;const view=new EditorView({parent:host,state:EditorState.create({doc:initialValueRef.current,extensions:[basicSetup,languageForPath(path),EditorView.lineWrapping,EditorView.contentAttributes.of({"aria-label":"远程文本内容","aria-multiline":"true"}),EditorView.updateListener.of((update)=>{if(update.docChanged)onChangeRef.current(update.state.doc.toString());}),EditorView.theme({"&":{height:"100%",backgroundColor:"var(--editor-bg)",color:"var(--editor-text)"},".cm-scroller":{fontFamily:"SFMono-Regular, Menlo, Monaco, monospace",fontSize:"12px",lineHeight:"1.55",overflow:"auto"},".cm-gutters":{backgroundColor:"var(--editor-gutter-bg)",color:"var(--editor-gutter-text)",borderRight:"1px solid var(--editor-border)"},".cm-activeLine,.cm-activeLineGutter":{backgroundColor:"var(--editor-active-bg)"},".cm-selectionBackground,.cm-content ::selection":{backgroundColor:"var(--editor-selection)!important"},".cm-cursor":{borderLeftColor:"var(--accent)"},".cm-foldPlaceholder":{backgroundColor:"var(--editor-active-bg)",border:"1px solid var(--editor-border)",color:"var(--editor-gutter-text)"}})]})});viewRef.current=view;return()=>{view.destroy();viewRef.current=null;};},[path]);
  useEffect(()=>{const view=viewRef.current;if(!view)return;const current=view.state.doc.toString();if(current!==value)view.dispatch({changes:{from:0,to:current.length,insert:value}});},[value]);
  return <div className="remote-code-editor" ref={hostRef}/>;
});
RemoteCodeEditor.displayName="RemoteCodeEditor";

function languageForPath(path:string):Extension{const name=path.toLowerCase();if(name.endsWith(".json")||name.endsWith(".jsonc"))return json();if(/\.(ya?ml)$/.test(name))return yaml();if(/\.(tsx?|jsx?|mjs|cjs)$/.test(name))return javascript({typescript:/\.tsx?$/.test(name),jsx:/\.[jt]sx$/.test(name)});if(name.endsWith(".py"))return python();if(/\.(md|markdown)$/.test(name))return markdown();if(/\.(html?|vue|svelte)$/.test(name))return html();if(/\.(css|scss|less)$/.test(name))return css();if(name.endsWith(".sql"))return sql();if(/\.(sh|bash|zsh)$/.test(name)||name.endsWith("/bashrc")||name.endsWith("/zshrc"))return StreamLanguage.define(shell);return[];}
