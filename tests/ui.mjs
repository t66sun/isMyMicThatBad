import assert from 'node:assert/strict';
import { chromium } from '../src/app/node_modules/playwright-core/index.mjs';
import { editorFlow, stored, beginDrag, movePointer, finishDrag } from './editor-flow.mjs';
const browser = await chromium.launch({executablePath:'C:/Program Files (x86)/Microsoft/Edge/Application/msedge.exe',headless:true});
const page = await browser.newPage({viewport:{width:1100,height:760},deviceScaleFactor:1.5});
const errors=[];page.on('pageerror',e=>errors.push(e.message));page.on('console',m=>{if(m.type()==='error')errors.push(m.text());});
try {
 await page.goto('http://127.0.0.1:1420/');
 assert.equal(await page.title(),'isMyMicThatBad · 音频工作台');
 assert.equal(new URL(page.url()).host,'127.0.0.1:1420');
 const result=await editorFlow(page);
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
 await page.screenshot({path:process.argv[4],fullPage:true});
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
