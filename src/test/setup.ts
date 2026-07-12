import "@testing-library/jest-dom/vitest";
import { cleanup } from "@testing-library/react";
import { afterEach } from "vitest";

if(!Range.prototype.getClientRects)Range.prototype.getClientRects=()=>({length:0,item:()=>null,[Symbol.iterator]:function*(){}} as DOMRectList);
if(!Range.prototype.getBoundingClientRect)Range.prototype.getBoundingClientRect=()=>({x:0,y:0,width:0,height:0,top:0,right:0,bottom:0,left:0,toJSON:()=>({})});

afterEach(cleanup);
