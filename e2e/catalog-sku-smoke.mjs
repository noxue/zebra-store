// Built storefront only; every request is intercepted and no backend is contacted.
import assert from 'node:assert/strict';
import fs from 'node:fs/promises';
import path from 'node:path';
import {fileURLToPath} from 'node:url';
import {chromium} from 'playwright';
const root=path.dirname(fileURLToPath(import.meta.url));
const dist=path.resolve(root,'../storefront/dist');
const artifactsBase=process.env.BROWSER_ARTIFACT_DIR || path.join(root,'test-results/catalog-sku');
const artifacts=path.join(artifactsBase,'run-'+new Date().toISOString().replaceAll(':','-'));
await fs.mkdir(artifacts,{recursive:true});
await fs.writeFile(path.join(artifacts,'run.json'),JSON.stringify({status:'running'})+'\n');
const launch=process.env.BROWSER_EXECUTABLE ? {executablePath:process.env.BROWSER_EXECUTABLE} : {};
const browser=await chromium.launch({headless:true,...launch});
const envelope=data=>({status_code:0,msg:'ok',data});
const rows=[
 {id:17,sku_code:'PLAN-17',spec_values:{周期:'2–10天',地区:'示例地区 A'},price_amount:'8.00',upstream_stock:8},
 {id:29,sku_code:'PLAN-29',spec_values:{周期:'11–30天',地区:'示例地区 A'},price_amount:'12.50',upstream_stock:5},
 {id:41,sku_code:'PLAN-41',spec_values:{周期:'2–10天',地区:'示例地区 B'},price_amount:'19.00',upstream_stock:6},
].map(row=>({...row,is_active:true,stock_status:'in_stock'}));
const product={id:7,slug:'guided',title:{'zh-CN':'示例接码套餐'},description:{'zh-CN':'请选择实际地区与周期，核对套餐后加入购物车。'},price_amount:'8.00',purchase_type:'guest',fulfillment_type:'upstream',stock_display_mode:'exact',stock_status:'in_stock',skus:rows};
const results=[];
try {
 for(const width of [360,390,1280]) {
  const context=await browser.newContext({viewport:{width,height:900},locale:'zh-CN',reducedMotion:'reduce',serviceWorkers:'block'});
  const unexpected=[],errors=[],writes=[];
  let includeDetails=false;
  await context.route('**/*',async route=>{
   const request=route.request(),url=new URL(request.url());
   if(url.hostname!=='catalog-fixture.test'){unexpected.push(url.origin);await route.abort();return;}
   if(request.method()!=='GET'){writes.push(url.pathname);await route.fulfill({status:405,body:'Fixture is read-only'});return;}
   if(url.pathname.startsWith('/api/v1/')){
    let data;
    if(url.pathname==='/api/v1/public/config')data={brand:{site_name:'示例商店'},currency:'CNY',languages:['zh-CN'],theme:{effects:{sakura:false,sparkle:false}},captcha:{provider:'none'}};
    else if(url.pathname==='/api/v1/public/products/guided')data={...product,content:includeDetails?{'zh-CN':'<p>示例套餐使用说明，用于验证长详情页面滚动后仍能清楚看到购买状态。</p>'.repeat(30)}:null};
    else if(['/api/v1/public/categories','/api/v1/public/member-levels','/api/v1/public/posts','/api/v1/public/banners'].includes(url.pathname))data=[];
    else{unexpected.push(url.pathname);await route.fulfill({status:404,body:'Unknown fixture API'});return;}
    await route.fulfill({status:200,contentType:'application/json',body:JSON.stringify(envelope(data))});return;
   }
   const asset=url.pathname.startsWith('/assets/')||url.pathname==='/favicon.svg';
   const file=asset?path.resolve(dist,'.'+url.pathname):path.join(dist,'index.html');
   assert(file.startsWith(dist+path.sep));
   const type={'.html':'text/html','.js':'text/javascript','.css':'text/css','.svg':'image/svg+xml','.woff2':'font/woff2'}[path.extname(file)]||'application/octet-stream';
   await route.fulfill({status:200,contentType:type,body:await fs.readFile(file)});
  });
  const page=await context.newPage();page.on('pageerror',e=>errors.push(e.message));
  try {
   await page.goto('http://catalog-fixture.test/products/guided');
   await page.getByText('选择套餐后显示价格',{exact:true}).waitFor();
   assert(await page.getByRole('button',{name:'立即购买',exact:true}).isDisabled());
   if(width<1024){
    await page.evaluate(()=>window.scrollTo({top:document.documentElement.scrollHeight,behavior:'instant'}));
    const bar=page.getByRole('region',{name:'快捷购买'});
    const shortGeometry=await page.getByRole('button',{name:'立即购买',exact:true}).boundingBox();
    assert(shortGeometry && shortGeometry.y>=0 && shortGeometry.y+shortGeometry.height<=900);
    assert.equal(await bar.isVisible(),false,'Short pages retain visible purchase controls');
    includeDetails=true;await page.reload();
    await page.getByText('选择套餐后显示价格',{exact:true}).waitFor();
    await page.evaluate(()=>window.scrollTo({top:document.documentElement.scrollHeight,behavior:'instant'}));
    await bar.waitFor({state:'visible'});
    assert((await bar.innerText()).includes('请选择套餐'));assert(!(await bar.innerText()).includes('8.00'));
    assert(await bar.getByRole('button',{name:'立即购买',exact:true}).isDisabled());
    await page.evaluate(()=>window.scrollTo({top:0,behavior:'instant'}));
    await page.waitForFunction(()=>scrollY===0);await bar.waitFor({state:'hidden'});
    await page.evaluate(()=>{window.__nativeEndScroll=new Promise(resolve=>document.addEventListener('scrollend',()=>resolve(true),{once:true}));});
    await page.keyboard.press('End');await page.evaluate(()=>window.__nativeEndScroll);
    const afterEnd=await page.getByRole('button',{name:'立即购买',exact:true}).first().boundingBox();
    assert(afterEnd && afterEnd.y+afterEnd.height<=0,'Keyboard scrolling must move the original actions above the viewport');
    await bar.waitFor({state:'visible'});
    assert((await bar.innerText()).includes('请选择套餐'));
    await page.evaluate(()=>window.scrollTo({top:0,behavior:'instant'}));
    await page.waitForFunction(()=>scrollY===0);await bar.waitFor({state:'hidden'});
   }
   const period=page.locator('fieldset').filter({has:page.getByText('周期',{exact:true})});
   const region=page.locator('fieldset').filter({has:page.getByText('地区',{exact:true})});
   await period.getByRole('button',{name:'11–30天',exact:true}).click();
   assert.equal(await region.getByRole('button',{name:'示例地区 B',exact:true}).count(),0);
   await region.getByRole('button',{name:'示例地区 A',exact:true}).click();
   assert(await page.getByRole('button',{name:'立即购买',exact:true}).isEnabled());
   assert((await page.locator('body').innerText()).includes('12.50 CNY'));
   await period.getByRole('button',{name:'2–10天',exact:true}).focus();await page.keyboard.press('Enter');
   assert(await page.getByRole('button',{name:'立即购买',exact:true}).isDisabled());
   await region.getByRole('button',{name:'示例地区 B',exact:true}).focus();await page.keyboard.press('Space');
   await page.getByRole('button',{name:'加入购物车',exact:true}).click();
   await page.waitForFunction(()=>JSON.parse(localStorage.getItem('cart_items')||'[]').length===1);
   const cart=await page.evaluate(()=>JSON.parse(localStorage.getItem('cart_items')));
   assert.equal(cart[0].skuId,41);assert.equal(cart[0].priceAmount,'19.00');assert.equal(cart[0].quantity,1);
   await page.evaluate(()=>document.fonts.ready);
   const metrics=await page.evaluate(()=>({width:innerWidth,scroll:document.documentElement.scrollWidth,controls:[...document.querySelectorAll('fieldset button')].map(e=>e.getBoundingClientRect().height)}));
   assert(metrics.scroll<=metrics.width+1,JSON.stringify(metrics));assert(metrics.controls.every(h=>h>=43.5));
   assert.deepEqual(errors,[]);assert.deepEqual(unexpected,[]);assert.deepEqual(writes,[]);
   await page.evaluate(()=>window.scrollTo({top:0,behavior:'instant'}));
   await page.waitForFunction(()=>scrollY===0);
   await page.screenshot({path:path.join(artifacts,`sku-${width}.png`),fullPage:true});
   results.push({width,actualSkuId:cart[0].skuId,price:cart[0].priceAmount,overflow:false,financialWrites:0,keyboard:true});
  } catch(error) {
   await page.screenshot({path:path.join(artifacts,`failed-${width}.png`),fullPage:true});
   await fs.writeFile(path.join(artifacts,'failure.json'),JSON.stringify({width,error:String(error),errors,unexpected,writes,geometry:await page.evaluate(()=>({scrollY,scrollHeight:document.documentElement.scrollHeight,buttons:[...document.querySelectorAll('button')].filter(e=>e.textContent.trim()==='立即购买').map(e=>({top:e.getBoundingClientRect().top,bottom:e.getBoundingClientRect().bottom}))}))},null,2)+'\n');
   throw error;
  } finally {await context.close();}
 }
 await fs.writeFile(path.join(artifacts,'results.json'),JSON.stringify(results,null,2)+'\n');
 await fs.writeFile(path.join(artifacts,'run.json'),JSON.stringify({status:'passed',results},null,2)+'\n');
 console.log(JSON.stringify({artifacts,results}));
} catch(error) {
 await fs.writeFile(path.join(artifacts,'run.json'),JSON.stringify({status:'failed',error:String(error),results},null,2)+'\n');
 throw error;
} finally {await browser.close();}
