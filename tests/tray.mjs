import assert from 'node:assert/strict';
import {writeFile, readFile} from 'node:fs/promises';
import {execFileSync} from 'node:child_process';
import {resolve} from 'node:path';
const endpoint=process.env.WEBDRIVER_URL??'http://127.0.0.1:9515';
const [binaryArg,settingsFile,resultFile]=process.argv.slice(2);
const binary=resolve(binaryArg);
let session, pid;
const results={};
async function request(path,method='POST',body={}) {
  const response=await fetch(endpoint+path,{method,headers:{'Content-Type':'application/json'},...(method==='GET'?{}:{body:JSON.stringify(body)})});
  const data=await response.json();if(!response.ok||data.value?.error)throw Error(JSON.stringify(data.value));return data.value;
}
const cmd=(path,body={})=>request('/session/'+session+path,'POST',body);
const js=(fn,...args)=>cmd('/execute/async',{script:'const done=arguments[arguments.length-1];Promise.resolve(('+fn.toString()+')(...Array.from(arguments).slice(0,-1))).then(value=>done({value}),error=>done({failure:String(error)}));',args}).then(r=>{if(r.failure)throw Error(r.failure);return r.value;});
const invoke=(command,args={})=>js((command,args)=>window.__TAURI_INTERNALS__.invoke(command,args),command,args);
const pause=ms=>new Promise(r=>setTimeout(r,ms));
async function wait(fn){for(let i=0;i<50;i++){if(await fn())return;await pause(100);}throw Error('Tray condition timed out');}
const click=label=>js(label=>document.querySelector('button[aria-label="'+label+'"]').click(),label);
const tray=action=>execFileSync('pwsh.exe',['-NoProfile','-STA','-File','tests/Tray-Action.ps1','-Action',action],{windowsHide:true,timeout:30000});
const running=()=>{try{process.kill(pid,0);return true;}catch{return false;}};
async function launch(){session=(await request('/session','POST',{capabilities:{alwaysMatch:{browserName:'webview2','ms:edgeOptions':{binary}}}})).sessionId;await wait(()=>js(()=>Boolean(window.__TAURI_INTERNALS__&&document.querySelector('.library-card')&&!document.querySelector('.transport').disabled)));pid=Number(execFileSync('powershell.exe',['-NoProfile','-Command',"(Get-Process | Where-Object { $_.Path -eq '"+binary.replaceAll("'","''")+"' }).Id"],{windowsHide:true}).toString().trim());assert.ok(pid>0,'isolated native process exists');}
async function toggle(){await click('打开设置');const expected=await js(()=>{const input=Array.from(document.querySelectorAll('label')).find(n=>n.textContent==='关闭窗口后进入托盘').querySelector('input');const expected=!input.checked;input.click();return expected;});await wait(()=>js(expected=>Array.from(document.querySelectorAll('label')).find(n=>n.textContent==='关闭窗口后进入托盘').querySelector('input').checked===expected&&!document.querySelector('.transport').disabled,expected));await click('关闭设置');}
try {
  await launch();
  const original=(await invoke('load_session')).session;
  assert.equal(original.closeToTray,false,'legacy configuration defaults off');
  const devices=await invoke('list_devices');
  const input=devices.find(d=>d.input&&/Intel/i.test(d.name))??devices.find(d=>d.input&&d.isDefault&&!d.name.includes('CABLE'));
  const output=devices.find(d=>!d.input&&d.name.includes('CABLE Input'));
  assert.ok(input&&output,'real microphone and VB-CABLE');
  await invoke('save_session',{session:{...original,inputId:input.id,outputId:output.id,chain:[],monitoring:false}});
  await cmd('/refresh');await wait(()=>js(()=>Boolean(document.querySelector('.library-card')&&!document.querySelector('.transport').disabled)));
  await toggle();
  assert.equal(JSON.parse(await readFile(settingsFile,'utf8')).closeToTray,true);
  await cmd('/refresh');await wait(()=>js(()=>Boolean(document.querySelector('.library-card')&&!document.querySelector('.transport').disabled)));
  await click('打开设置');assert.equal(await js(()=>Array.from(document.querySelectorAll('label')).find(n=>n.textContent==='关闭窗口后进入托盘').querySelector('input').checked),true);await click('关闭设置');
  await js(()=>document.querySelector('.transport').click());await wait(async()=>(await invoke('get_status')).running);
  await click('关闭窗口');await wait(async()=>!(await invoke('plugin:window|is_visible',{label:'main'})));
  await pause(1000);const hidden=await invoke('get_status');assert.equal(hidden.running,true);assert.equal(hidden.error,null);results.hiddenAudio=hidden;
  tray('Processing');await wait(async()=>!(await invoke('get_status')).running);
  tray('Processing');await wait(async()=>(await invoke('get_status')).running);
  results.trayProcessingToggle=true;
  tray('Click');await wait(()=>invoke('plugin:window|is_visible',{label:'main'}));assert.equal((await invoke('get_status')).running,true);
  await invoke('plugin:window|close',{label:'main'});await wait(async()=>!(await invoke('plugin:window|is_visible',{label:'main'})));
  tray('Show');await wait(()=>invoke('plugin:window|is_visible',{label:'main'}));assert.equal((await invoke('get_status')).running,true);
  await click('最小化窗口');tray('Click');await wait(async()=>!(await invoke('plugin:window|is_minimized',{label:'main'})));
  await writeFile(resultFile.replace(/\.json$/,'.png'),Buffer.from(await request('/session/'+session+'/screenshot','GET'),'base64'));
  tray('Exit');await wait(()=>!running());await request('/session/'+session,'DELETE');session=null;
  results.trayRestoreAndExit=true;
  await launch();assert.equal((await invoke('load_session')).session.closeToTray,true);assert.equal((await invoke('get_status')).running,false);
  const saved=(await invoke('load_session')).session;
  await click('关闭窗口');await wait(async()=>!(await invoke('plugin:window|is_visible',{label:'main'})));
  await wait(()=>js(()=>document.querySelector('.app-footer').textContent.includes('配置已保存')));
  await invoke('save_session',{session:{...saved,inputId:'missing-test-device'}});
  tray('Processing');
  await wait(()=>invoke('plugin:window|is_visible',{label:'main'}));
  assert.equal((await invoke('get_status')).running,false);
  await wait(()=>js(()=>document.querySelector('[role="alert"]')?.textContent.includes('设备')));
  results.startFailureRestoresWindow=true;
  await invoke('save_session',{session:saved});await cmd('/refresh');await wait(()=>js(()=>!document.querySelector('.transport').disabled));
  await toggle();assert.equal((await invoke('load_session')).session.closeToTray,false);
  await click('关闭窗口');await wait(()=>!running());await request('/session/'+session,'DELETE');session=null;
  results.defaultClose=true;results.persistedOption=true;
  await writeFile(resultFile,JSON.stringify(results,null,2));console.log(JSON.stringify(results));
} finally {if(session)await request('/session/'+session,'DELETE').catch(()=>{});}
