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
   const ordinaryDefinition=catalog.find(definition=>definition.type==='distortion');
   assert.ok(ordinaryDefinition&&(ordinaryDefinition.summaryKeys?.length??ordinaryDefinition.parameters.length)>=2,'the ordinary comparison card has a two-value summary');
   await page.getByRole('button',{name:'添加'+ordinaryDefinition.name,exact:true}).click();await settle(page);
   const ordinaryCard=page.locator('.effect-card[aria-label="'+ordinaryDefinition.name+'滤镜"]');
   assert.equal(await ordinaryCard.count(),1);
   for(const type of ['highpass','gain','bitcrusher','frequency']){
     const definition=catalog.find(item=>item.type===type);assert.ok(definition,`catalog includes ${type}`);
     await page.getByRole('button',{name:'添加'+definition.name,exact:true}).click();await settle(page);
   }
   const alignmentTypes=['rvc','highpass','distortion','gain','bitcrusher','frequency'];
   const alignmentCards=alignmentTypes.map(type=>{
     const definition=catalog.find(item=>item.type===type);assert.ok(definition,`catalog includes ${type}`);
     return {type,definition,card:page.locator('.effect-card[aria-label="'+definition.name+'滤镜"]')};
   });
   for(const item of alignmentCards)assert.equal(await item.card.count(),1,`${item.type} card is present in the six-card chain`);
   const readCardGeometry=card=>card.evaluate(element=>{
     const cardRect=element.getBoundingClientRect(),viewportRect=document.querySelector('.chain-viewport').getBoundingClientRect();
     const rect=node=>{const box=node.getBoundingClientRect();return {left:box.left-cardRect.left,right:cardRect.right-box.right,top:box.top-cardRect.top,width:box.width,height:box.height};};
     const select=element.querySelector('.card-select'),icon=select.querySelector(':scope > svg');
     const name=element.querySelector('.card-name'),values=element.querySelector('.card-values'),actions=element.querySelector('.card-actions');
     if(!icon||!name||!values||!actions)throw Error('filter card header, summary, or actions are missing');
     return {
       width:cardRect.width,height:cardRect.height,cardVertical:{top:cardRect.top-viewportRect.top,bottom:viewportRect.bottom-cardRect.bottom},padding:getComputedStyle(select).padding,
       icon:rect(icon),name:rect(name),titleGap:name.getBoundingClientRect().top-icon.getBoundingClientRect().bottom,values:rect(values),actions:rect(actions),
       actionButtons:Array.from(actions.querySelectorAll(':scope > button')).map(rect),
       summaryLines:values.children.length,bypassed:Boolean(element.querySelector('.bypass-label')),
       texts:Array.from(element.querySelectorAll('.card-name,.card-values,.bypass-label')).map(rect),
     };
   });
   const assertNear=(actual,expected,message,tolerance=2)=>assert.ok(Math.abs(actual-expected)<=tolerance,`${message}: ${actual} vs ${expected}`);
   const assertCardContentsFit=async(card,bypassed,label)=>{
     const geometry=await readCardGeometry(card);
     assert.equal(geometry.bypassed,bypassed,`${label} bypass label matches card state`);
     for(const text of geometry.texts){
       assert.ok(text.left>=-1&&text.right>=-1,`${label} text stays inside card horizontally`);
       assert.ok(text.top+text.height<=geometry.height+1,`${label} text stays inside card vertically`);
     }
   };
   const expectedSummaryLines={rvc:2,highpass:1,distortion:2,gain:1,bitcrusher:2,frequency:1};
   const assertAllCardGeometry=async viewport=>{
     const measurements=await Promise.all(alignmentCards.map(async item=>({item,geometry:await readCardGeometry(item.card)})));
     const reference=measurements[0].geometry;
     for(const {item,geometry} of measurements){
       const label=`${item.type} card at ${viewport}`;
       assertNear(geometry.width,152,`${label} width`);assertNear(geometry.height,158,`${label} height`);
       assert.ok(geometry.cardVertical.top>=-1&&geometry.cardVertical.bottom>=-1,`${label} stays fully inside the chain viewport vertically`);
       assert.equal(geometry.padding,reference.padding,`${label} shares card padding`);
       assert.equal(geometry.summaryLines,expectedSummaryLines[item.type],`${label} has the catalog summary row count`);
       assertNear(geometry.icon.left,reference.icon.left,`${label} icon left`);
       assertNear(geometry.icon.top,reference.icon.top,`${label} icon top`);
       assertNear(geometry.name.left,reference.name.left,`${label} title left`);
       assertNear(geometry.name.top,reference.name.top,`${label} title top is stable for single and double summaries`);
       assertNear(geometry.values.top,reference.values.top,`${label} summary top is stable for single and double summaries`);
       assertNear(geometry.titleGap,6,`${label} icon-to-title gap`,1);
       for(const dimension of ['top','right'])assertNear(geometry.actions[dimension],reference.actions[dimension],`${label} action group ${dimension}`);
       assert.equal(geometry.actionButtons.length,2,`${label} has both shared actions`);
       for(let index=0;index<2;index++)for(const dimension of ['left','right','top','width','height'])
         assertNear(geometry.actionButtons[index][dimension],reference.actionButtons[index][dimension],`${label} action ${index} ${dimension}`);
       await assertCardContentsFit(item.card,false,label);
     }
   };
   const readPaneGeometry=()=>page.evaluate(()=>{
     const rect=element=>{const box=element.getBoundingClientRect();return {left:box.left,top:box.top,width:box.width,height:box.height,bottom:box.bottom};};
     return {inspector:rect(document.querySelector('.inspector')),fields:rect(document.querySelector('.inspector-fields')),
       chain:rect(document.querySelector('.chain-panel')),viewport:rect(document.querySelector('.chain-viewport')),footer:rect(document.querySelector('footer'))};
   });
   const assertPaneGeometryStable=(actual,expected,viewport,type)=>{
     for(const region of ['inspector','fields','chain','viewport','footer'])for(const dimension of ['left','top','width','height','bottom'])
       assertNear(actual[region][dimension],expected[region][dimension],`${region} ${dimension} stays fixed after selecting ${type} at ${viewport}`,1);
   };
   const selectAlignmentCard=async item=>{
     await page.getByRole('button',{name:'选择'+item.definition.name+'滤镜',exact:true}).click();await settle(page);
     assert.equal(await item.card.locator('.card-select').getAttribute('aria-pressed'),'true',`${item.type} selection is exposed`);
     assert.ok(await item.card.evaluate(card=>card.classList.contains('selected')),`${item.type} uses the shared selected state`);
   };
   const assertLastParameterReachable=async item=>{
     const lastParameter=page.locator('.inspector-fields .parameter').last();
     assert.ok(await lastParameter.count(),`${item.type} inspector has its final parameter`);
     await lastParameter.scrollIntoViewIfNeeded();
     assert.ok(await lastParameter.evaluate(parameter=>{
       const box=parameter.getBoundingClientRect(),fields=document.querySelector('.inspector-fields').getBoundingClientRect();
       return box.top>=fields.top-1&&box.bottom<=fields.bottom+1;
     }),`${item.type} final parameter scrolls fully into the inspector view`);
   };
   await page.getByRole('button',{name:'选择AI变声 (RVC)滤镜',exact:true}).click();await settle(page);
   assert.equal(await aiCard.locator('.card-select').getAttribute('aria-pressed'),'true','AI card selection is exposed');
   assert.ok(await aiCard.evaluate(card=>card.classList.contains('selected')),'selected AI card uses the shared selected state');
   for(const viewport of [{width:1100,height:760},{width:900,height:640},{width:390,height:844},{width:390,height:640}]){
     await page.setViewportSize(viewport);
     await settle(page);
     const viewportLabel=`${viewport.width}x${viewport.height}`;
     await assertAllCardGeometry(viewportLabel);
     const expectedHeight=viewport.height<=700?142:viewport.width<=640?168:180;
     let baseline;
     for(const item of alignmentCards){
       await selectAlignmentCard(item);
       const current=await readPaneGeometry();
       assertNear(current.inspector.height,expectedHeight,`inspector height at ${viewportLabel}`);
       assert.ok(current.inspector.bottom<=current.footer.top+1,`inspector does not cover the footer at ${viewportLabel}`);
       assert.ok(current.chain.bottom<=current.inspector.top+1,`chain panel ends before the inspector at ${viewportLabel}`);
       assert.ok(current.viewport.bottom<=current.inspector.top+1,`chain cards and horizontal scrollbar stay above the inspector at ${viewportLabel}`);
       assert.ok(current.footer.bottom<=viewport.height+1,`footer remains inside ${viewportLabel}`);
       assert.ok(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth),`six-card layout has no page overflow at ${viewportLabel}`);
       if(!baseline)baseline=current;
       else assertPaneGeometryStable(current,baseline,viewportLabel,item.type);
       await assertLastParameterReachable(item);
     }
   }
   await page.setViewportSize({width:1100,height:760});
 await page.getByRole('button',{name:'旁路AI变声 (RVC)',exact:true}).click();await settle(page);
   await assertCardContentsFit(aiCard,true,'AI');
   await assertCardContentsFit(ordinaryCard,false,'ordinary');
 await page.getByRole('button',{name:'启用AI变声 (RVC)',exact:true}).click();await settle(page);
   await page.getByRole('button',{name:'旁路'+ordinaryDefinition.name,exact:true}).click();await settle(page);
   await assertCardContentsFit(aiCard,false,'AI');
   await assertCardContentsFit(ordinaryCard,true,'ordinary');
   await page.getByRole('button',{name:'启用'+ordinaryDefinition.name,exact:true}).click();await settle(page);
   const longModelPath='C:\\Users\\Toshu\\Documents\\RVC Models\\'+('unusually-long-folder-name\\'.repeat(10))+'Saisho-compatible-voice-model-with-a-long-name.pth';
   const modelSession=await stored(page);modelSession.chain.find(node=>node.type==='rvc').model={path:longModelPath,index:''};
   await page.evaluate(session=>localStorage.setItem('mic-preview',JSON.stringify(session)),modelSession);
   await page.reload();await settle(page);
   await page.getByRole('button',{name:'选择AI变声 (RVC)滤镜',exact:true}).click();await settle(page);
   const modelPathField=page.locator('.ai-path').first();
   assert.equal(await modelPathField.getAttribute('title'),longModelPath,'long model path remains available in its tooltip');
   assert.equal(await modelPathField.textContent(),longModelPath,'long model path remains present in the saved session view');
   const aiStyle=await page.locator('.ai-controls').evaluate(root=>{
     const buttonStyles=Array.from(root.querySelectorAll('.secondary-button')).map(button=>{
       const style=getComputedStyle(button);return {fontFamily:style.fontFamily,fontSize:style.fontSize,padding:style.padding,borderRadius:style.borderRadius,backgroundColor:style.backgroundColor,borderColor:style.borderColor};
     });
     const hint=root.querySelector('.hint'),sharedHint=document.querySelector('.settings-dialog .hint');
     const style=element=>{const value=getComputedStyle(element);return {fontFamily:value.fontFamily,fontSize:value.fontSize,lineHeight:value.lineHeight,color:value.color};};
     return {buttons:buttonStyles,buttonClasses:Array.from(root.querySelectorAll('button')).map(button=>button.classList.contains('secondary-button')),
       hint:style(hint),sharedHint:style(sharedHint),
       sections:Array.from(root.querySelectorAll('.ai-environment,.ai-model')).map(element=>element.getBoundingClientRect().toJSON())};
   });
   assert.equal(aiStyle.buttons.length,5,'all AI action buttons use the shared secondary control');
   assert.ok(aiStyle.buttonClasses.every(Boolean),'AI action buttons use the shared secondary-button class');
   assert.ok(aiStyle.buttons.every(style=>JSON.stringify(style)===JSON.stringify(aiStyle.buttons[0])),'AI action buttons share one control style');
   assert.deepEqual(aiStyle.hint,aiStyle.sharedHint,'AI prompts reuse the settings hint typography and color');
   assertNear(aiStyle.sections[0].top,aiStyle.sections[1].top,'desktop AI sections share a row');
   assertNear(aiStyle.sections[0].width,aiStyle.sections[1].width,'desktop AI sections share the available width');
   for(const viewport of [{width:900,height:640},{width:390,height:844}]){
     await page.setViewportSize(viewport);await settle(page);
     const fit=await page.evaluate(()=>{
       const fields=document.querySelector('.inspector-fields'),ai=document.querySelector('.ai-controls'),path=document.querySelector('.ai-path'),footer=document.querySelector('footer');
       const fr=fields.getBoundingClientRect(),ar=ai.getBoundingClientRect(),pr=path.getBoundingClientRect();
       return {viewportWidth:innerWidth,documentWidth:document.documentElement.scrollWidth,fieldsRight:fr.right,aiRight:ar.right,pathRight:pr.right,
         pathTitle:path.title,
         buttons:Array.from(ai.querySelectorAll('button')).map(button=>{const r=button.getBoundingClientRect();return {left:r.left,right:r.right};}),
         sections:Array.from(ai.querySelectorAll('.ai-environment,.ai-model')).map(element=>{const r=element.getBoundingClientRect();return {top:r.top,bottom:r.bottom,left:r.left,right:r.right};}),
         footerBottom:footer.getBoundingClientRect().bottom};
     });
     assert.ok(fit.documentWidth<=fit.viewportWidth,'long AI model path does not create horizontal page overflow');
     assert.ok(fit.aiRight<=fit.fieldsRight+1&&fit.pathRight<=fit.fieldsRight+1,'AI content and model path stay within the parameter panel');
     assert.equal(fit.pathTitle,longModelPath);
     for(const button of fit.buttons)assert.ok(button.left>=0&&button.right<=fit.fieldsRight+1,'AI control button fits the panel width');
     assert.ok(fit.footerBottom<=viewport.height+1,'footer stays visible with AI controls');
     if(viewport.width<640)assert.ok(fit.sections[1].top>=fit.sections[0].bottom-1,'AI sections stack cleanly in a narrow window');
     else assertNear(fit.sections[0].top,fit.sections[1].top,'AI sections remain side by side at tablet width');
   }
   const fields=page.locator('.inspector-fields');
   assert.ok(await fields.evaluate(element=>element.scrollHeight>element.clientHeight),'AI controls and parameters fit inside a scrollable inspector');
   await page.locator('.ai-controls .ai-buttons').last().scrollIntoViewIfNeeded();
   assert.ok(await page.getByRole('button',{name:'导入 .index 索引',exact:true}).evaluate(button=>{const box=button.getBoundingClientRect(),parent=document.querySelector('.inspector-fields').getBoundingClientRect();return box.top>=parent.top-1&&box.bottom<=parent.bottom+1;}),'AI import buttons remain reachable inside the inspector');
   const lastParameter=page.locator('.inspector-fields .parameter').last();
   await lastParameter.scrollIntoViewIfNeeded();
   assert.ok(await lastParameter.evaluate(parameter=>{const box=parameter.getBoundingClientRect(),parent=document.querySelector('.inspector-fields').getBoundingClientRect();return box.top>=parent.top-1&&box.bottom<=parent.bottom+1;}),'last AI parameter remains reachable inside the inspector');
   assert.ok(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth),'AI controls keep the narrow page free of horizontal overflow');
   await page.setViewportSize({width:1100,height:760});await settle(page);
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
 console.log(JSON.stringify({status:'passed',...result,uiAlignment:'six-card single/double summaries, selected-state geometry, fixed inspector and scroll reachability',layout:'1100x760, 900x640, 390x844, 390x640; reduced motion; modal focus',environment:'Edge/Playwright, production preview; Browser plugin unavailable'},null,2));
}finally{await browser.close();}
