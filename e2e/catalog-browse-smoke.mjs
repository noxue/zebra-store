// Built UI against isolated catalog fixtures. Every request is intercepted, including errors.
import assert from 'node:assert/strict';
import fs from 'node:fs/promises';
import path from 'node:path';
import {fileURLToPath} from 'node:url';
import {chromium} from 'playwright';
const root=path.dirname(fileURLToPath(import.meta.url)),dist=path.resolve(root,'../storefront/dist');
const base=process.env.BROWSER_ARTIFACT_DIR || path.join(root,'test-results/catalog-browse');
const artifacts=path.join(base,'run-'+new Date().toISOString().replaceAll(':','-'));
await fs.mkdir(artifacts,{recursive:true});
await fs.writeFile(path.join(artifacts,'run.json'),JSON.stringify({status:'running'})+'\n');
const browser=await chromium.launch({headless:true,...(process.env.BROWSER_EXECUTABLE?{executablePath:process.env.BROWSER_EXECUTABLE}:{})});
const origin='http://catalog-browse.test',US='provider:country:000Ab';
const dimensions=['family','project','country','city','ip_type','line'];
const envelope=data=>({status_code:0,msg:'ok',data});
function deferred(){let resolve;const promise=new Promise(r=>resolve=r);return{promise,resolve};}
async function within(promise,label){let timer;try{return await Promise.race([promise,new Promise((_,reject)=>{timer=setTimeout(()=>reject(new Error(label+' timed out')),30000);})]);}finally{clearTimeout(timer);}}
function product(id,slug,title,metadata,category=1){return{id,slug,title:{'zh-CN':title},metadata,category_id:category,category:{id:category,parent_id:0,slug:category===1?'ip':'sms',name:{'zh-CN':category===1?'IP服务':'接码服务'}},price_amount:'10.00',purchase_type:'guest',fulfillment_type:'upstream',stock_display_mode:'exact',stock_status:'in_stock',skus:[{id:id*10,sku_code:'PLAN-'+id,spec_values:{周期:'30天'},price_amount:'10.00',is_active:true,upstream_stock:10,stock_status:'in_stock'}]};}
const initial=[
 product(101,'us-ip','美国IP服务',{family:'ip',country:US,city:'city:us01',ip_type:'native',line:'NONE'}),
 product(102,'ca-ip','加拿大IP服务',{family:'ip',country:'CA',city:'city:ca01',ip_type:'broadcast',line:'RECOMMENDED'}),
 product(103,'sms','接码项目服务',{family:'sms',country:US,project:'OPENAI'},2),
 product(104,'nz-ip','新地区服务',{family:'ip',country:'NZ',city:'city:nz01',ip_type:'native',line:'NONE'}),
 ...Array.from({length:22},(_,i)=>product(200+i,'region-'+i,'地区套餐'+i,{family:'ip',country:'A'+String(i).padStart(2,'0'),city:'city:a'+i,ip_type:'native',line:'NONE'})),
];
const results=[];
try {
 for(const mode of ['card','list'])for(const width of [360,390,1280]){
  const context=await browser.newContext({viewport:{width,height:900},locale:'zh-CN',reducedMotion:'reduce',serviceWorkers:'block'});
  const state={rename:false,addRegion:false,failProducts:false,failOptions:false,delayOld:false,productReads:0};
  const oldSeen=deferred(),releaseOld=deferred();
  const errors=[],unexpected=[],writes=[],requests=[];
  const catalog=()=>state.addRegion?[...initial,product(399,'new-region','新增地区服务',{family:'ip',country:'region:NEW.01',city:'city:new01',ip_type:'native',line:'NONE'})]:initial;
  const label=(dimension,id)=>({family:{ip:'静态IP',sms:'接码'},project:{OPENAI:'OpenAI'},ip_type:{native:'原生IP',broadcast:'广播IP'},line:{NONE:'普通线路',RECOMMENDED:'推荐线路'},country:{[US]:state.rename?'美国（更新名称）':'美国',CA:'加拿大',NZ:'新西兰','region:NEW.01':'新增地区'},city:{'city:us01':'美国城市','city:ca01':'多伦多','city:nz01':'奥克兰','city:new01':'新增城市'}}[dimension]?.[id] || '示例地区'+id);
  const matching=(query,except)=>catalog().filter(row=>{
   if(query.get('category_id')&&row.category_id!==Number(query.get('category_id')))return false;
   if(query.get('search')&&!row.title['zh-CN'].includes(query.get('search')))return false;
   return dimensions.every(key=>key===except||!query.get('facet_'+key)||row.metadata[key]===query.get('facet_'+key));
  });
  const json=async(route,data,status=200)=>route.fulfill({status,contentType:'application/json',body:JSON.stringify(data)});
  await context.route('**/*',async route=>{
   const req=route.request(),url=new URL(req.url()),q=url.searchParams;
   if(url.origin!==origin){unexpected.push(url.origin);await route.abort();return;}
   if(req.method()!=='GET'){writes.push(url.pathname);await json(route,{status_code:405,msg:'Read-only fixture',data:null},405);return;}
   if(url.pathname.startsWith('/api/v1/')){
    requests.push({path:url.pathname,params:Object.fromEntries(q)});
    if(url.pathname==='/api/v1/public/config'){await json(route,envelope({brand:{site_name:'示例商店'},currency:'CNY',template_mode:mode,languages:['zh-CN'],theme:{effects:{sakura:false,sparkle:false}},captcha:{provider:'none'}}));return;}
    if(url.pathname==='/api/v1/public/categories'){await json(route,envelope([{id:1,parent_id:0,slug:'ip',name:{'zh-CN':'IP服务'}},{id:2,parent_id:0,slug:'sms',name:{'zh-CN':'接码服务'}}]));return;}
    if(url.pathname==='/api/v1/public/catalog-facets'){await json(route,envelope({schema_version:1,revision_format:'decimal_string',dimensions,supports_product_filters:true,supports_value_search:true}));return;}
    if(url.pathname==='/api/v1/public/catalog-facet-values'){
     if(state.failOptions&&q.get('option_search')==='加拿大'){await json(route,{status_code:429,msg:'SYNTHETIC_PRIVATE_RATE_RESPONSE',data:null},429);return;}
     const key=q.get('dimension'),groups=new Map();
     for(const row of matching(q,key)){
      const id=row.metadata[key];if(!id)continue;
      const value=groups.get(id)||{value_id:id,label:{'zh-CN':label(key,id)},product_count:0};value.product_count++;groups.set(id,value);
     }
     let items=[...groups.values()].sort((a,b)=>a.value_id<b.value_id?-1:a.value_id>b.value_id?1:0);
     if(q.has('value_id'))items=items.filter(row=>row.value_id===q.get('value_id'));
     if(q.get('option_search'))items=items.filter(row=>row.label['zh-CN'].includes(q.get('option_search')));
     const page=Number(q.get('page')||1),size=Number(q.get('page_size')||20),total=items.length;
     await json(route,{...envelope(items.slice((page-1)*size,page*size)),pagination:{page,page_size:size,total,total_page:Math.ceil(total/size)}});return;
    }
    if(url.pathname==='/api/v1/public/products'){
     state.productReads++;
     if(state.failProducts){await json(route,{status_code:503,msg:'SYNTHETIC_PRIVATE_PRODUCT_RESPONSE',data:null},503);return;}
     if(state.delayOld&&q.get('search')==='旧请求'){
      oldSeen.resolve();await releaseOld.promise;
      await json(route,{...envelope([{...initial[0],title:{'zh-CN':'旧请求商品'}}]),pagination:{page:1,page_size:Number(q.get('page_size')||20),total:1,total_page:1}});return;
     }
     const items=matching(q),page=Number(q.get('page')||1),size=Number(q.get('page_size')||20);
     await json(route,{...envelope(items.slice((page-1)*size,page*size)),pagination:{page,page_size:size,total:items.length,total_page:Math.ceil(items.length/size)}});return;
    }
    if(['/api/v1/public/member-levels','/api/v1/public/posts','/api/v1/public/banners'].includes(url.pathname)){await json(route,envelope([]));return;}
    unexpected.push(url.pathname);await json(route,{status_code:404,data:null},404);return;
   }
   const file=url.pathname.startsWith('/assets/')||url.pathname==='/favicon.svg'?path.resolve(dist,'.'+url.pathname):path.join(dist,'index.html');
   assert(file.startsWith(dist+path.sep));
   const type={'.html':'text/html','.js':'text/javascript','.css':'text/css','.svg':'image/svg+xml','.woff2':'font/woff2'}[path.extname(file)]||'application/octet-stream';
   await route.fulfill({status:200,contentType:type,body:await fs.readFile(file)});
  });
  const page=await context.newPage();page.on('pageerror',error=>errors.push(error.message));
  const controls=()=>page.getByRole('region',{name:'按条件查找',exact:true});
  const open=async()=>{if(width<1024){await page.getByRole('button',{name:'筛选',exact:true}).click();await page.getByRole('dialog',{name:'筛选',exact:true}).waitFor({state:'visible'});}await controls().waitFor({state:'visible'});};
  const close=async()=>{if(width<1024){await controls().getByRole('button',{name:'查看商品',exact:true}).click();await page.getByRole('dialog',{name:'筛选',exact:true}).waitFor({state:'hidden'});}};
  const frames=()=>page.evaluate(()=>new Promise(resolve=>requestAnimationFrame(()=>requestAnimationFrame(resolve))));
  try {
   const deep=origin+'/products?facet_family=ip&facet_country='+encodeURIComponent(US)+'&campaign=keep';
   await page.goto(deep);await page.getByText('美国IP服务',{exact:true}).waitFor();await open();
   await controls().getByText('美国',{exact:true}).waitFor();
   assert.equal(new URL(page.url()).searchParams.get('facet_country'),US);
   assert(!(await controls().innerText()).includes(US));
   if(width<1024){
    const dialog=page.getByRole('dialog',{name:'筛选',exact:true}),first=dialog.locator('button').first();
    await page.waitForFunction(()=>document.activeElement===document.querySelector('[role="dialog"] button'));
    await page.keyboard.press('Shift+Tab');assert.equal(await page.evaluate(()=>document.activeElement.textContent.trim()),'查看商品');
    await page.keyboard.press('Tab');assert(await first.evaluate(el=>el===document.activeElement));
    await page.keyboard.press('Escape');await dialog.waitFor({state:'hidden'});
    assert(await page.getByRole('button',{name:'筛选',exact:true}).evaluate(el=>el===document.activeElement));
    assert.equal(await page.evaluate(()=>document.body.style.overflow),'');await open();
   }
   // The exact selected value is not in the first twenty country options.
   // legend is not a consistently mapped ARIA role, so use its text for the stable field boundary.
   const field=(name)=>controls().locator('fieldset').filter({has:page.locator('legend').filter({hasText:new RegExp('^'+name+'$')})});
   await field('国家或地区').getByRole('button',{name:'加载更多',exact:true}).click();
   await field('国家或地区').getByRole('button',{name:/^美国\s*1项商品$/}).waitFor();
   await field('城市').getByRole('button',{name:/^美国城市/}).click();
   await controls().getByLabel('搜索国家或地区',{exact:true}).fill('加拿大');
   await field('国家或地区').getByRole('button',{name:/^加拿大/}).click();await close();
   await page.getByText('加拿大IP服务',{exact:true}).waitFor();
   await page.waitForFunction(()=>new URL(location.href).searchParams.get('facet_country')==='CA');
   assert.equal(new URL(page.url()).searchParams.get('facet_city'),null);
   assert.equal(new URL(page.url()).searchParams.get('campaign'),'keep');
   await open();await controls().getByRole('button',{name:'清除条件',exact:true}).click();await close();
   const keyword=()=>page.getByRole('textbox',{name:'搜索商品',exact:true});
   await keyword().fill('新地区');await page.getByText('新地区服务',{exact:true}).waitFor();
   assert.equal(new URL(page.url()).searchParams.get('facet_country'),null);
   state.failProducts=true;await keyword().fill('出错');
   await page.getByText('商品资料暂时不可用，请稍后重试。',{exact:true}).waitFor();
   assert(!(await page.locator('body').innerText()).includes('SYNTHETIC_PRIVATE_PRODUCT_RESPONSE'));
   state.failProducts=false;await page.getByRole('alert').getByRole('button',{name:'重试',exact:true}).click();
   await keyword().fill('');await page.getByText('美国IP服务',{exact:true}).waitFor();
   await open();state.failOptions=true;await controls().getByLabel('搜索国家或地区',{exact:true}).fill('加拿大');
   await field('国家或地区').getByText('请求过于频繁，请稍后重试。',{exact:true}).waitFor();
   assert(!(await field('国家或地区').innerText()).includes('未找到匹配选项'));
   state.failOptions=false;await field('国家或地区').getByRole('button',{name:'重试',exact:true}).click();
   await field('国家或地区').getByRole('button',{name:/^加拿大/}).waitFor();await close();
   state.delayOld=true;await keyword().fill('旧请求');await within(oldSeen.promise,'Delayed product request');
   await keyword().fill('新地区');await page.getByText('新地区服务',{exact:true}).waitFor();
   const oldCompleted=page.waitForResponse(response=>new URL(response.url()).searchParams.get('search')==='旧请求');
   releaseOld.resolve();await (await oldCompleted).finished();await frames();
   assert(await page.getByText('新地区服务',{exact:true}).isVisible());assert.equal(await page.getByText('旧请求商品',{exact:true}).count(),0);
   state.rename=true;state.addRegion=true;await page.goto(deep);
   await page.getByText('美国IP服务',{exact:true}).waitFor();await open();
   await controls().getByText('美国（更新名称）',{exact:true}).waitFor();
   assert.equal(new URL(page.url()).searchParams.get('facet_country'),US);
   await controls().getByLabel('搜索国家或地区',{exact:true}).fill('新增地区');
   await field('国家或地区').getByRole('button',{name:/^新增地区/}).click();await close();
   await page.getByText('新增地区服务',{exact:true}).waitFor();
   assert.equal(new URL(page.url()).searchParams.get('facet_country'),'region:NEW.01');
   await page.evaluate(()=>document.fonts.ready);
   const metrics=await page.evaluate(()=>({width:innerWidth,scroll:document.documentElement.scrollWidth,ids:[...document.querySelectorAll('input[id]')].map(el=>el.id)}));
   assert(metrics.scroll<=metrics.width+1,JSON.stringify(metrics));assert.equal(new Set(metrics.ids).size,metrics.ids.length);
   if(width<1024){
    await open();await frames();
    const dialog=page.getByRole('dialog',{name:'筛选',exact:true});
    await dialog.evaluate(async el=>{await Promise.all(el.parentElement.getAnimations({subtree:true}).map(animation=>animation.finished.catch(()=>{})));});
    assert(await dialog.isVisible());assert.equal(await dialog.evaluate(el=>getComputedStyle(el.parentElement).opacity),'1');
    await page.screenshot({path:path.join(artifacts,`filters-${mode}-${width}.png`),fullPage:false});await close();
   }
   else await page.screenshot({path:path.join(artifacts,`filters-${mode}-${width}.png`),fullPage:true});
   state.productReads=0;await page.goto(origin+'/products?facet_country=A&facet_country=B');
   await page.getByTestId('catalog-list-feedback').getByText('筛选链接不完整或无效，请清除条件后重试。',{exact:true}).waitFor();
   assert.equal(state.productReads,0,'Invalid links must not silently fetch an unfiltered catalog');
   await page.getByTestId('catalog-list-feedback').getByRole('button',{name:'清除筛选条件',exact:true}).click();await page.getByText('美国IP服务',{exact:true}).waitFor();
   assert.equal(new URL(page.url()).searchParams.get('facet_country'),null);
   assert.deepEqual(errors,[]);assert.deepEqual(unexpected,[]);assert.deepEqual(writes,[]);
   results.push({mode,width,urlIdentity:true,paging:true,parentSwitch:true,errorsDistinct:true,staleResponseDiscarded:true,renameStable:true,newRegion:true,invalidUrlBlocked:true,overflow:false,financialWrites:0});
   await fs.writeFile(path.join(artifacts,'results.json'),JSON.stringify(results,null,2)+'\n');
  }catch(error){
   releaseOld.resolve();await page.screenshot({path:path.join(artifacts,`failed-${mode}-${width}.png`),fullPage:true});
   await fs.writeFile(path.join(artifacts,'failure.json'),JSON.stringify({mode,width,error:String(error),errors,unexpected,writes,requests},null,2)+'\n');throw error;
  }finally{await context.close();}
 }
 await fs.writeFile(path.join(artifacts,'run.json'),JSON.stringify({status:'passed',results},null,2)+'\n');console.log(JSON.stringify({artifacts,results}));
}catch(error){await fs.writeFile(path.join(artifacts,'run.json'),JSON.stringify({status:'failed',error:String(error),results},null,2)+'\n');throw error;}
finally{await browser.close();}
