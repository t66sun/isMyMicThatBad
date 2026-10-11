import assert from 'node:assert/strict';
import { chromium } from '../src/app/node_modules/playwright-core/index.mjs';
import { editorFlow, historyFlow, stored, raw, settle, beginDrag, movePointer, finishDrag } from './editor-flow.mjs';
import { readFile } from 'node:fs/promises';
const catalog=JSON.parse(await readFile(new URL('../src/app/src/filters.json',import.meta.url),'utf8'));
const browser = await chromium.launch({executablePath:'C:/Program Files (x86)/Microsoft/Edge/Application/msedge.exe',headless:true});
const page = await browser.newPage({viewport:{width:1100,height:760},deviceScaleFactor:1.5});
const errors=[];page.on('pageerror',e=>errors.push(e.message));page.on('console',m=>{if(m.type()==='error')errors.push(m.text());});
try {
 await page.goto('http://127.0.0.1:1420/');
 assert.equal(await page.title(),'isMyMicThatBad · 音频工作台');
 assert.equal(new URL(page.url()).host,'127.0.0.1:1420');
 const result=await editorFlow(page);
 await historyFlow(page);
 await raw(page);
 assert.equal(catalog.length,14);
 assert.equal(await page.locator('.library-card').count(),14);
 await page.getByRole('button',{name:'添加AI变声 (RVC)',exact:true}).click();await settle(page);
 const aiCard=page.locator('.effect-card[aria-label="AI变声 (RVC)滤镜"]');
 assert.equal(await aiCard.count(),1);
 const assertAiCardTextFits=async bypassed=>{
   const geometry=await aiCard.evaluate(card=>{
     const cardRect=card.getBoundingClientRect();
     const name=card.querySelector('.card-name'),values=card.querySelector('.card-values');
     if(!name||!values)throw Error('RVC card title or parameter summary is missing');
     return {
       lineHeight:Number.parseFloat(getComputedStyle(name).lineHeight),
       nameHeight:name.getBoundingClientRect().height,
       bypassVisible:Boolean(card.querySelector('.bypass-label')),
       texts:Array.from(card.querySelectorAll('.card-name,.card-values,.bypass-label')).map(element=>({
         selector:'.'+element.className,
         bottom:element.getBoundingClientRect().bottom,
       })),
       cardBottom:cardRect.bottom,
     };
   });
   assert.ok(geometry.nameHeight>=geometry.lineHeight-1,'RVC title has room for at least one CSS line');
   assert.equal(geometry.bypassVisible,bypassed,'RVC bypass label matches card state');
   for(const text of geometry.texts)assert.ok(text.bottom<=geometry.cardBottom+1,`${text.selector} stays inside the RVC card`);
 };
 await assertAiCardTextFits(false);
 await page.getByRole('button',{name:'旁路AI变声 (RVC)',exact:true}).click();await settle(page);
 await assertAiCardTextFits(true);
 await page.getByRole('button',{name:'启用AI变声 (RVC)',exact:true}).click();await settle(page);
 await raw(page);
 await page.locator('#filter-library').evaluate(n=>{n.scrollLeft=0;});await settle(page);
 await page.getByRole('button',{name:'向后浏览滤镜库',exact:true}).click();
 await page.waitForTimeout(250);
 assert.ok(await page.locator('#filter-library').evaluate(n=>n.scrollLeft>0));
 for(const type of ['gate','formant','eq','compressor','pitch','reverb','distortion','tremolo','bitcrusher']) {
   const definition=catalog.find(d=>d.type===type);
   await page.getByRole('button',{name:'添加'+definition.name,exact:true}).click();
   await settle(page);
   assert.equal(await page.locator('.inspector input[type="number"]').count(),definition.parameters.length);
   for(const parameter of definition.parameters) {
     const control=page.locator('.inspector').getByLabel(parameter.label,{exact:true});
     const value=Math.min(parameter.max,parameter.default+parameter.step);
     await control.fill(String(value));await control.press('Tab');await settle(page);
     assert.equal((await stored(page)).chain.at(-1).params[parameter.key],value);
   }
 }
 for(const name of ['原音','金属音','电话音','女声','炸麦','广播声','空灵声']) {
   await page.getByRole('button',{name:'声音预设',exact:true}).click();
   await page.getByRole('menuitem',{name:new RegExp('^'+name)}).click();await settle(page);
   const saved=await stored(page);await page.reload();await settle(page);
   assert.deepEqual((await stored(page)).chain,saved.chain);
 }
 await raw(page);
 await page.getByRole('button',{name:'添加增益',exact:true}).click();await settle(page);
 await page.screenshot({path:process.argv[2],fullPage:true});
 const box=await page.locator('.chain-viewport').boundingBox();
 await beginDrag(page,page.getByRole('button',{name:'选择增益滤镜',exact:true}));
 await movePointer(page,box.x+box.width/2,box.y+box.height/2-15);
 await page.screenshot({path:process.argv[3],fullPage:true});
 await page.keyboard.press('Escape');await finishDrag(page);
 for(const viewport of [{width:900,height:640},{width:390,height:844}]) {
   await page.setViewportSize(viewport);
   assert.ok(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth),'no body horizontal overflow');
   assert.ok(await page.evaluate(()=>document.querySelector('footer').getBoundingClientRect().bottom<=innerHeight+1),'footer remains visible');
 }
  await page.evaluate(()=>{const button=document.querySelector('button[aria-label="原音对比"]');button.disabled=false;button.setAttribute('aria-pressed','true');button.textContent='原音 · 点击恢复效果';});
  const compare=page.locator('button[aria-label="原音对比"]');
  assert.ok(await compare.evaluate(button=>{const panel=button.closest('.chain-panel'),rect=button.getBoundingClientRect(),bounds=panel.getBoundingClientRect();return rect.left>=bounds.left&&rect.right<=bounds.right&&button.scrollWidth<=button.clientWidth&&button.scrollHeight<=button.clientHeight;}),'pressed compare label stays visible at 390px');
  await page.screenshot({path:process.argv[4],fullPage:true});
  await page.evaluate(()=>{const button=document.querySelector('button[aria-label="原音对比"]');button.disabled=true;button.setAttribute('aria-pressed','false');button.textContent='对比原音';});
 await page.emulateMedia({reducedMotion:'reduce'});
 const before=(await stored(page)).chain;
 await beginDrag(page,page.getByRole('button',{name:'添加高通',exact:true}));
 assert.equal(await page.locator('.overlay-card').evaluate(n=>getComputedStyle(n).transform),'none');
 await page.keyboard.press('Escape');await finishDrag(page);
 assert.deepEqual((await stored(page)).chain,before);
 await page.getByRole('button',{name:'打开设置',exact:true}).click();
 await page.getByRole('dialog',{name:'设置',exact:true}).waitFor();
 await page.keyboard.press('Escape');
 await page.getByRole('button',{name:'打开设置',exact:true}).evaluate(n=>{if(document.activeElement!==n)throw Error('dialog focus was not restored');});
 assert.deepEqual(errors,[]);
 console.log(JSON.stringify({status:'passed',...result,layout:'1100x760, 900x640, 390x844; reduced motion; modal focus',environment:'Edge/Playwright, production preview; Browser plugin unavailable'},null,2));
}finally{await browser.close();}
