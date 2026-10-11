import assert from 'node:assert/strict';
import { writeFile, readFile } from 'node:fs/promises';
const endpoint=process.env.WEBDRIVER_URL ?? 'http://127.0.0.1:9515';
let session, original;
async function request(path, method='POST', body={}) {
 const response=await fetch(endpoint+path,{method,headers:{'Content-Type':'application/json'},...(method==='GET'?{}:{body:JSON.stringify(body)})});
 const data=await response.json();
 if(!response.ok||data.value?.error)throw Error(JSON.stringify(data.value));
 return data.value;
}
const cmd=(path,body={})=>request('/session/'+session+path,'POST',body);
const js=(fn,...args)=>cmd('/execute/async',{script:'const done=arguments[arguments.length-1]; Promise.resolve(('+fn.toString()+')(...Array.from(arguments).slice(0,-1))).then(value=>done({value}),error=>done({failure:String(error)}));',args}).then(result=>{if(result.failure)throw Error(result.failure);return result.value;});
const invoke=(command,args={})=>js((command,args)=>window.__TAURI_INTERNALS__.invoke(command,args),command,args);
const pause=ms=>new Promise(resolve=>setTimeout(resolve,ms));
const captureErrors=()=>js(()=>{window.testErrors=JSON.parse(sessionStorage.getItem('native-test-errors')??'[]');const record=message=>{testErrors.push(message);sessionStorage.setItem('native-test-errors',JSON.stringify(testErrors));};addEventListener('error',e=>record(e.message));addEventListener('unhandledrejection',e=>record(String(e.reason)));});
async function wait(fn) {for(let i=0;i<50;i++){if(await fn())return;await pause(100);}throw Error('Native condition timed out');}
const click=label=>js(label=>{const button=document.querySelector('button[aria-label="'+label+'"]');if(!button||button.disabled)throw Error('Unavailable '+label);button.click();},label);
const transport=()=>js(()=>document.querySelector('.transport').click());
const pointer=actions=>cmd('/actions',{actions:[{type:'pointer',id:'mouse',parameters:{pointerType:'mouse'},actions}]});
const move=(x,y,duration=100)=>({type:'pointerMove',origin:'viewport',x:Math.round(x),y:Math.round(y),duration});
try {
 const created=await request('/session','POST',{capabilities:{alwaysMatch:{browserName:'webview2','ms:edgeOptions':{binary:process.argv[3]}}}});
 session=created.sessionId;
 await wait(()=>js(()=>Boolean(window.__TAURI_INTERNALS__&&document.querySelector('.library-card'))));
 assert.equal(await request('/session/'+session+'/title','GET'),'isMyMicThatBad · 音频工作台');
 await captureErrors();
 original=(await invoke('load_session')).session;
 const devices=await invoke('list_devices');
 const input=devices.find(d=>d.input&&d.id===process.env.MIC_TEST_INPUT_ID)??devices.find(d=>d.input&&d.isDefault&&!d.name.includes('CABLE'));
 const output=devices.find(d=>!d.input&&d.name.includes('CABLE Input'));
 const monitor=devices.find(d=>!d.input&&d.isDefault&&!d.name.includes('CABLE'));
 assert.ok(input&&output&&monitor,'actual hardware endpoints exist');
 const configured={...original,inputId:input.id,outputId:output.id,monitorId:monitor.id,monitoring:false,bufferMs:5,chain:[],presets:[]};
 await invoke('save_session',{session:configured});await cmd('/refresh');
 await wait(()=>js(()=>Boolean(document.querySelector('.empty-chain'))));
 await captureErrors();
 for(const name of ['频率平移','高通','增益','低通','失真'])await click('添加'+name);
 await click('选择频率平移滤镜');
 await js(()=>{const n=document.querySelector('input[type="number"]');Object.getOwnPropertyDescriptor(HTMLInputElement.prototype,'value').set.call(n,'100');n.dispatchEvent(new Event('input',{bubbles:true}));n.focus();n.blur();});
 await pause(700);
 let saved=(await invoke('load_session')).session;
 assert.equal(saved.chain.length,5);
 assert.equal(saved.chain[0].params.shift_hz,100);
 assert.equal(JSON.parse(await readFile(process.argv[4],'utf8')).chain.length,5,'portable persistence exists');
 await click('声音预设');await js(()=>Array.from(document.querySelectorAll('[role="menuitem"]')).find(n=>n.textContent==='保存当前管线').click());
 await js(()=>{const n=document.getElementById('preset-name');Object.getOwnPropertyDescriptor(HTMLInputElement.prototype,'value').set.call(n,'原生界面验证');n.dispatchEvent(new Event('input',{bubbles:true}));});
 await click('保存预设');await pause(700);
 assert.ok((await invoke('load_session')).session.presets.some(p=>p.name==='原生界面验证'));
 await cmd('/refresh');await wait(()=>js(()=>document.querySelectorAll('.effect-card').length===5));
 await captureErrors();
 assert.equal((await invoke('load_session')).session.chain[0].id,saved.chain[0].id);
 const rejected=await js(async()=>{try{await window.__TAURI_INTERNALS__.invoke('set_chain',{filters:[{id:'invalid',type:'frequency',enabled:true,params:{shift_hz:2001}}]});return false;}catch{return true;}});
 assert.ok(rejected,'native parameter validation retained');
 await click('打开设置');
 assert.equal(await js(()=>document.querySelector('.settings-dialog').open),true);
 await click('关闭设置');
 await transport();await wait(async()=>(await invoke('get_status')).running);
 await click('打开设置');await js(()=>document.querySelector('.monitor-switch input').click());
 await wait(async()=>(await invoke('get_status')).monitoring);
 await js(()=>document.querySelector('.monitor-switch input').click());
 await wait(async()=>!(await invoke('get_status')).monitoring);await click('关闭设置');
 saved=(await invoke('load_session')).session;
 const geometry=await js(()=>{const a=document.querySelectorAll('.effect-card');const r=a[0].querySelector('.card-select').getBoundingClientRect(),t=a[2].getBoundingClientRect();return {x:r.x+r.width/2,y:r.y+r.height/2,tx:t.right-8,ty:t.y+t.height/2};});
 await js(()=>{window.dragFrames=[];let previous=performance.now();window.frameProbe=true;const step=time=>{if(!frameProbe)return;dragFrames.push(time-previous);previous=time;requestAnimationFrame(step);};requestAnimationFrame(step);});
 await pointer([move(geometry.x,geometry.y),{type:'pointerDown',button:0},move(geometry.x+9,geometry.y)]);
 await wait(()=>js(()=>Boolean(document.querySelector('.overlay-card'))));
 await pointer([move(geometry.tx,geometry.ty,400)]);
 await pause(700);
 assert.deepEqual((await invoke('load_session')).session.chain,saved.chain,'native drag preview does not persist');
 const during=await invoke('get_status');assert.equal(during.running,true);assert.equal(during.error,null);
 await pointer([{type:'pointerUp',button:0}]);await pause(700);
 assert.equal((await invoke('load_session')).session.chain[2].id,saved.chain[0].id,'native drop commits');
 const frames=await js(()=>{frameProbe=false;return dragFrames.slice(2).sort((a,b)=>a-b);});
 const frameP95=frames[Math.floor(frames.length*.95)];
 await click('旁路频率平移');await wait(async()=>!(await invoke('load_session')).session.chain[2].enabled);
 assert.equal((await invoke('load_session')).session.chain[2].enabled,false);
 await transport();await wait(async()=>!(await invoke('get_status')).running);
 for(const name of ['原音','金属音','电话音','女声','炸麦','广播声','空灵声']) {
   await click('声音预设');
   await js(name=>Array.from(document.querySelectorAll('[role="menuitem"]')).find(n=>n.textContent.startsWith(name)).click(),name);
   await pause(700);const preset=(await invoke('load_session')).session;
   await cmd('/refresh');await wait(()=>js(()=>Boolean(document.querySelector('.library-card'))));
   await captureErrors();assert.deepEqual((await invoke('load_session')).session.chain,preset.chain);
   await transport();await wait(async()=>(await invoke('get_status')).running);await pause(500);
   const startup=await invoke('get_status');await pause(2000);
   const status=await invoke('get_status');console.log(JSON.stringify({preset:name,startup,status}));assert.equal(status.error,null);assert.equal(status.underruns,startup.underruns,name+' must not accumulate underruns after warmup');assert.equal(status.overflows,0,name+' overflows');
   await transport();await wait(async()=>!(await invoke('get_status')).running);
 }
 assert.equal(await js(()=>document.querySelectorAll('.library-card').length),12);
 for(const name of ['噪声门','共振峰移位','均衡器','压缩器','音高移位','混响','颤音']) {
   await click('添加'+name);await pause(600);
   await js(()=>{for(const n of document.querySelectorAll('.inspector input[type="number"]')) {
     const value=Math.min(Number(n.max),Number(n.value)+Number(n.step));
     Object.getOwnPropertyDescriptor(HTMLInputElement.prototype,'value').set.call(n,String(value));
     n.dispatchEvent(new Event('input',{bubbles:true}));n.focus();n.blur();
   }});await pause(700);
 }
 await transport();await wait(async()=>(await invoke('get_status')).running);
 await transport();await wait(async()=>!(await invoke('get_status')).running);
 await click('最大化窗口');await wait(()=>invoke('plugin:window|is_maximized',{label:'main'}));
 await click('还原窗口');await wait(async()=>!(await invoke('plugin:window|is_maximized',{label:'main'})));
 const shot=await request('/session/'+session+'/screenshot','GET');await writeFile(process.argv[2],Buffer.from(shot,'base64'));
 assert.deepEqual(await js(()=>testErrors),[]);
 assert.equal(await js(()=>document.querySelectorAll('[role="alert"]').length),0);
 await invoke('save_session',{session:original});original=null;await cmd('/refresh');await pause(700);
 await click('最小化窗口');await wait(()=>invoke('plugin:window|is_minimized',{label:'main'}));
 console.log(JSON.stringify({status:'passed',checks:'real portable persistence/preset/reload, native validation, hardware start/stop/restart, monitoring, running drag preview/drop, bypass, maximize/restore/minimize',during,frame_samples:frames.length,frame_p95_ms:frameP95,driver:'Edge WebDriver over local HTTP'},null,2));
 await click('关闭窗口');
}catch(error){throw error;}finally{
 if(session&&original){await invoke('stop_audio').catch(()=>{});await invoke('save_session',{session:original}).catch(()=>{});}
 if(session)await request('/session/'+session,'DELETE').catch(()=>{});
}
