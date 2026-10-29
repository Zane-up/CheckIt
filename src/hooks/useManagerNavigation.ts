import { useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { api, type ManagerNavigationRequest } from "@/services/api";

export function useManagerNavigation() {
  const [request,setRequest]=useState<ManagerNavigationRequest|null>(null);
  const newest=useRef(0);
  useEffect(()=>{
    let disposed=false;let stop:(()=>void)|undefined;
    const read=async()=>{
      try { const value=await api.getManagerNavigation();
        if(!disposed && value && value.id>newest.current){newest.current=value.id;setRequest(value);}
      } catch(e){console.error("Manager navigation unavailable",e);}
    };
    void listen("manager-navigation-changed",()=>void read()).then(unlisten=>{
      if(disposed)unlisten();else{stop=unlisten;void read();}
    }).catch(()=>void read());
    void read();
    const interval=window.setInterval(()=>void read(),2000);
    return()=>{disposed=true;stop?.();window.clearInterval(interval);};
  },[]);
  useEffect(()=>{if(request)void api.acknowledgeManagerNavigation(request.id).catch(e=>console.error("Navigation acknowledgement failed",e));},[request]);
  return {request,clearRequest:(id:number)=>setRequest(old=>old?.id===id?null:old)};
}
